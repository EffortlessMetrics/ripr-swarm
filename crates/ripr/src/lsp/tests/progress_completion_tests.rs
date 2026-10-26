//! Deterministic progress preconditions for the zero-Git-timeout wire test.
use super::*;
use tokio::sync::{mpsc, oneshot};
use tower_lsp_server::LspServiceBuilder;

pub(super) type Publication = (oneshot::Receiver<u64>, oneshot::Sender<()>);
const ARM_PUBLICATION: &str = "test/armRefreshPublication";

/// This route exists only on the test service. Production still detaches
/// advisory create/Begin and never waits for a client's progress handshake.
pub(super) fn with_publication_barrier(
    builder: LspServiceBuilder<Backend>,
) -> (LspServiceBuilder<Backend>, mpsc::Receiver<Publication>) {
    let (sender, receiver) = mpsc::channel(1);
    let builder = builder.custom_method(ARM_PUBLICATION, move |backend: &Backend| {
        let installed = backend.install_refresh_publication_barrier_for_test();
        let sender = sender.clone();
        async move {
            let channels = installed.map_err(tower_lsp_server::jsonrpc::Error::invalid_params)?;
            sender
                .send(channels)
                .await
                .map_err(|_| tower_lsp_server::jsonrpc::Error::internal_error())?;
            Ok::<(), tower_lsp_server::jsonrpc::Error>(())
        }
    });
    (builder, receiver)
}

pub(super) async fn arm_publication<R, W>(
    reader: &mut R,
    writer: &mut W,
    receiver: &mut mpsc::Receiver<Publication>,
    id: u64,
) -> Result<Publication, String>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    tokio::time::timeout(Duration::from_mins(1), async {
        write_lsp_message(
            writer,
            serde_json::json!({"jsonrpc":"2.0","id":id,"method":ARM_PUBLICATION}),
        )
        .await?;
        let (response, pending) =
            read_lsp_response_with_notifications_and_progress(reader, writer, id).await?;
        if response.get("error").is_some() {
            return Err(format!("could not arm publication: {response}"));
        }
        if pending
            .iter()
            .any(|message| message["method"] == "$/progress")
        {
            return Err(format!(
                "progress remained after the preceding settled run: {pending:?}"
            ));
        }
        receiver
            .try_recv()
            .map_err(|error| format!("publication was not armed: {error}"))
    })
    .await
    .map_err(|error| format!("arming publication did not complete: {error}"))?
}

/// Require Begin for the barrier's generation before allowing publication.
/// A command response alone is acceptance, not a notification-drain barrier.
/// End is deliberately NOT a stop condition: after the committed status and
/// outcome log, the caller's unchanged exactly-one oracle must reject both
/// a missing End and duplicate Ends instead of silently accepting the first.
pub(super) async fn read_started_refresh_until_settled<R, W>(
    reader: &mut R,
    writer: &mut W,
    publication: Publication,
    response_id: Option<u64>,
    expected_status: &str,
) -> Result<(u64, Vec<serde_json::Value>), String>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    tokio::time::timeout(Duration::from_mins(1), async {
        let (mut witness, release) = publication;
        let mut generation: Option<u64> = None;
        let mut release = Some(release);
        let mut messages: Vec<serde_json::Value> = Vec::new();
        loop {
            if let Some(generation) = generation {
                let attempt = generation.to_string();
                let snapshot = format!("snapshot:{generation}");
                release_matching_begin(&messages, generation, &mut release)?;
                let status = messages.iter().rposition(|message|
                    message["method"] == "ripr/analysisStatus"
                        && message["params"]["attempt_id"].as_str() == Some(attempt.as_str())
                        && message["params"]["snapshot_id"].as_str() == Some(snapshot.as_str())
                        && message["params"]["run_status"] == expected_status
                        && message["params"]["state"] == "succeeded");
                // Outcome logs carry no generation. A log before the matching
                // committed status cannot close this generation's window.
                let outcome = status.is_some_and(|index| messages[index + 1..].iter().any(|message|
                    message["method"] == "window/logMessage"
                        && message["params"]["message"].as_str().is_some_and(|text|
                            text.contains("ripr analysis refresh attempt outcome=published"))));
                let response = response_id.is_none() || messages.iter().any(|message|
                    message.get("method").is_none()
                        && message.get("id").and_then(serde_json::Value::as_u64) == response_id);
                if release.is_none() && outcome && response {
                    return Ok((generation, messages));
                }
            }
            // Drain the wire even before the witness arrives: status or create
            // traffic must not fill the pipe and prevent reaching publication.
            let message = {
                let frame = read_lsp_message(reader);
                tokio::pin!(frame);
                loop {
                    tokio::select! {
                        reached = &mut witness, if generation.is_none() => {
                            let reached = reached.map_err(|error| format!("publication barrier dropped: {error}"))?;
                            generation = Some(reached);
                            release_matching_begin(&messages, reached, &mut release)?;
                        }
                        message = &mut frame => break message?,
                    }
                }
            };
            if message["method"] == "window/workDoneProgress/create" {
                let id = message.get("id").cloned().ok_or("create request carried no id")?;
                write_lsp_message(writer, serde_json::json!({"jsonrpc":"2.0","id":id,"result":null})).await?;
            }
            if message.get("method").is_none() && response_id.is_some()
                && message.get("id").and_then(serde_json::Value::as_u64) == response_id
                && message.get("error").is_some()
            {
                return Err(format!("refresh command failed: {message}"));
            }
            messages.push(message);
        }
    })
    .await
    .map_err(|error| format!("started refresh did not settle: {error}"))?
}

fn release_matching_begin(
    messages: &[serde_json::Value],
    generation: u64,
    release: &mut Option<oneshot::Sender<()>>,
) -> Result<(), String> {
    let token = format!("ripr-analysis-{generation}");
    if release.is_some()
        && messages.iter().any(|message| {
            message["method"] == "$/progress"
                && message["params"]["token"].as_str() == Some(token.as_str())
                && message["params"]["value"]["kind"] == "begin"
        })
    {
        release
            .take()
            .ok_or("missing publication release")?
            .send(())
            .map_err(|_| "publication release dropped".to_string())?;
    }
    Ok(())
}

/// Signals only after a real partial frame has been consumed. The witness
/// then arrives mid-read, discriminating a cancellation-unsafe select loop.
struct PrefixWitnessReader<R> {
    inner: R,
    armed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    consumed: Option<oneshot::Sender<()>>,
}

impl<R: AsyncRead + Unpin> AsyncRead for PrefixWitnessReader<R> {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let result = std::pin::Pin::new(&mut self.inner).poll_read(cx, buf);
        if buf.filled().len() > before
            && self.armed.load(std::sync::atomic::Ordering::SeqCst)
            && let Some(consumed) = self.consumed.take()
        {
            let _ = consumed.send(());
        }
        result
    }
}

pub(super) async fn run_started_refresh<R, W>(
    reader: &mut R,
    writer: &mut W,
    publication: Publication,
    id: u64,
    expected_status: &str,
) -> Result<(u64, Vec<serde_json::Value>), String>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    write_lsp_message(
        writer,
        serde_json::json!({"jsonrpc":"2.0","id":id,
        "method":"workspace/executeCommand","params":{"command":REFRESH_COMMAND,"arguments":[]}}),
    )
    .await?;
    read_started_refresh_until_settled(reader, writer, publication, Some(id), expected_status).await
}

async fn ordered_case(
    ends: &[u64],
    response_first: bool,
    witness_first: bool,
) -> Result<Vec<serde_json::Value>, String> {
    let (client, mut peer) = tokio::io::duplex(64 * 1024);
    let (reader, mut writer) = tokio::io::split(client);
    let armed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (consumed, prefix_read) = oneshot::channel();
    let mut reader = PrefixWitnessReader {
        inner: reader,
        armed: std::sync::Arc::clone(&armed),
        consumed: Some(consumed),
    };
    let (witness, reached) = oneshot::channel();
    let (release, mut released) = oneshot::channel();
    let ends = ends.to_vec();
    let producer = tokio::spawn(async move {
        let command = read_lsp_message(&mut peer).await?;
        assert_eq!(command["id"], 2);
        let mut witness = Some(witness);
        if witness_first {
            witness
                .take()
                .ok_or("missing witness")?
                .send(7)
                .map_err(|_| "witness dropped")?;
        }
        write_lsp_message(
            &mut peer,
            serde_json::json!({"jsonrpc":"2.0","id":99,
            "method":"window/workDoneProgress/create","params":{"token":"ripr-analysis-7"}}),
        )
        .await?;
        assert_eq!(read_lsp_message(&mut peer).await?["id"], 99);
        // The ACK of this second create proves the reader consumed the wrong
        // Begin first. No polling window is needed to check it did not release.
        write_lsp_message(
            &mut peer,
            serde_json::json!({"jsonrpc":"2.0","method":"$/progress",
            "params":{"token":"ripr-analysis-8","value":{"kind":"begin"}}}),
        )
        .await?;
        write_lsp_message(
            &mut peer,
            serde_json::json!({"jsonrpc":"2.0","id":100,
            "method":"window/workDoneProgress/create","params":{"token":"probe"}}),
        )
        .await?;
        assert_eq!(read_lsp_message(&mut peer).await?["id"], 100);
        assert!(
            matches!(
                released.try_recv(),
                Err(oneshot::error::TryRecvError::Empty)
            ),
            "a different generation's Begin released publication"
        );
        write_lsp_message(
            &mut peer,
            serde_json::json!({"jsonrpc":"2.0","method":"$/progress",
            "params":{"token":"ripr-analysis-7","value":{"kind":"begin"}}}),
        )
        .await?;
        if !witness_first {
            write_lsp_message(
                &mut peer,
                serde_json::json!({"jsonrpc":"2.0","id":101,
                "method":"window/workDoneProgress/create","params":{"token":"witness-probe"}}),
            )
            .await?;
            assert_eq!(read_lsp_message(&mut peer).await?["id"], 101);
            assert!(
                matches!(
                    released.try_recv(),
                    Err(oneshot::error::TryRecvError::Empty)
                ),
                "wire Begin replaced the authoritative publication witness"
            );
            let body = serde_json::to_vec(
                &serde_json::json!({"jsonrpc":"2.0","method":"window/logMessage",
                "params":{"type":3,"message":"partial-frame witness probe"}}),
            )
            .map_err(|error| error.to_string())?;
            let header = format!("Content-Length: {}\r\n\r\n", body.len());
            armed.store(true, std::sync::atomic::Ordering::SeqCst);
            peer.write_all(&header.as_bytes()[..3])
                .await
                .map_err(|error| error.to_string())?;
            prefix_read
                .await
                .map_err(|_| "partial frame was not consumed")?;
            witness
                .take()
                .ok_or("missing witness")?
                .send(7)
                .map_err(|_| "witness dropped")?;
            released.await.map_err(|_| "release dropped")?;
            peer.write_all(&header.as_bytes()[3..])
                .await
                .map_err(|error| error.to_string())?;
            peer.write_all(&body)
                .await
                .map_err(|error| error.to_string())?;
        } else {
            released.await.map_err(|_| "release dropped")?;
        }
        let response = serde_json::json!({"jsonrpc":"2.0","id":2,"result":null});
        if response_first {
            write_lsp_message(&mut peer, response.clone()).await?;
        }
        for generation in [8, 7] {
            write_lsp_message(&mut peer, serde_json::json!({"jsonrpc":"2.0","method":"ripr/analysisStatus",
                "params":{"attempt_id":generation.to_string(),"snapshot_id":format!("snapshot:{generation}"),
                "state":"succeeded","run_status":"limited"}})).await?;
            if generation == 7 {
                for end in &ends {
                    write_lsp_message(
                        &mut peer,
                        serde_json::json!({"jsonrpc":"2.0","method":"$/progress",
                        "params":{"token":format!("ripr-analysis-{end}"),"value":{"kind":"end"}}}),
                    )
                    .await?;
                }
            }
            write_lsp_message(
                &mut peer,
                serde_json::json!({"jsonrpc":"2.0","method":"window/logMessage",
                "params":{"type":3,"message":"ripr analysis refresh attempt outcome=published"}}),
            )
            .await?;
        }
        if !response_first {
            write_lsp_message(&mut peer, response).await?;
        }
        Ok::<(), String>(())
    });
    write_lsp_message(
        &mut writer,
        serde_json::json!({"jsonrpc":"2.0","id":2,
        "method":"workspace/executeCommand","params":{"command":REFRESH_COMMAND,"arguments":[]}}),
    )
    .await?;
    let (generation, messages) = read_started_refresh_until_settled(
        &mut reader,
        &mut writer,
        (reached, release),
        Some(2),
        "limited",
    )
    .await?;
    producer.await.map_err(|error| error.to_string())??;
    assert_eq!(generation, 7);
    assert!(
        messages
            .iter()
            .any(|message| message.get("method").is_none() && message["id"] == 2)
    );
    assert_eq!(
        analysis_status_params(&messages)
            .last()
            .ok_or("missing status")?["attempt_id"],
        "7"
    );
    Ok(messages)
}

fn exactly_one_matching_end(messages: &[serde_json::Value]) -> bool {
    let ends = messages
        .iter()
        .filter(|message| {
            message["method"] == "$/progress" && message["params"]["value"]["kind"] == "end"
        })
        .collect::<Vec<_>>();
    ends.len() == 1 && ends[0]["params"]["token"] == "ripr-analysis-7"
}

#[test]
fn started_refresh_window_keeps_both_response_orders_and_generation() -> Result<(), String> {
    work_done_progress_runtime()?.block_on(async {
        for response_first in [false, true] {
            for witness_first in [false, true] {
                let messages = ordered_case(&[7], response_first, witness_first).await?;
                assert!(exactly_one_matching_end(&messages));
            }
        }
        Ok(())
    })
}

#[test]
fn started_refresh_window_does_not_hide_missing_duplicate_or_wrong_end() -> Result<(), String> {
    work_done_progress_runtime()?.block_on(async {
        for ends in [&[][..], &[7, 7][..], &[8][..]] {
            let messages = ordered_case(ends, true, true).await?;
            assert!(!exactly_one_matching_end(&messages), "accepted {ends:?}");
        }
        Ok(())
    })
}
