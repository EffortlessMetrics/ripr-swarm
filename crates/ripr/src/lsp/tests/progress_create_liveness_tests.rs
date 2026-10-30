use super::*;

/// The client advertises progress but withholds the create response. The
/// real refresh must finish well before the production 30-second request
/// timeout. A missing base makes analysis fail deterministically without
/// depending on fixture-scale analysis speed.
async fn withheld_create_exchange(reply_late: bool) -> Result<(), String> {
    let root = unique_lsp_test_root("withheld-progress-create")?;
    let (client_io, server_io) = tokio::io::duplex(64 * 1024);
    let (mut reader, mut writer) = tokio::io::split(client_io);
    let (server_read, server_write) = tokio::io::split(server_io);
    let backend_root = root.path().to_path_buf();
    let (service, socket) =
        LspService::new(move |client| Backend::new(client, backend_root.clone()));
    let server = tokio::spawn(async move {
        Server::new(server_read, server_write, socket)
            .serve(service)
            .await;
    });
    // Abort/reap the owned task even when an assertion fails.
    let exchange = async {
        write_lsp_message(
            &mut writer,
            serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {
                    "processId": null,
                    "rootUri": file_uri_for_path(root.path())?.as_str(),
                    "initializationOptions": {"baseRef": "ripr-progress-missing-base"},
                    "capabilities": {"window": {"workDoneProgress": true}}
                }
            }),
        )
        .await?;
        let initialize = read_lsp_response(&mut reader, 1).await?;
        if initialize.get("error").is_some() {
            return Err(format!("SETUP: initialize failed: {initialize}"));
        }
        write_lsp_message(
            &mut writer,
            serde_json::json!({
                "jsonrpc": "2.0", "method": "initialized", "params": {}
            }),
        )
        .await?;
        write_lsp_message(
            &mut writer,
            serde_json::json!({
                "jsonrpc": "2.0", "id": 2, "method": "workspace/executeCommand",
                "params": {"command": REFRESH_COMMAND, "arguments": []}
            }),
        )
        .await?;

        let mut create = None;
        let mut saw_failed_attempt = false;
        let started = std::time::Instant::now();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let message = read_lsp_message(&mut reader).await?;
                match message["method"].as_str() {
                    Some("window/workDoneProgress/create") => {
                        if create.is_some() {
                            return Err(format!("duplicate create: {message}"));
                        }
                        create = Some(message.clone());
                        // Deliberately do not answer this request.
                    }
                    Some("$/progress") => return Err(format!(
                        "unanswered create emitted progress: {message}"
                    )),
                    Some("ripr/analysisStatus") => {
                        saw_failed_attempt |= message["params"]["state"] == "failed";
                    }
                    _ => {}
                }
                if message["id"] == 2 && message.get("method").is_none() {
                    if message.get("error").is_some() {
                        return Err(format!("refresh command failed: {message}"));
                    }
                    return Ok::<(), String>(());
                }
            }
        }).await.map_err(|_elapsed| format!(
            "refresh blocked by unanswered create after {:?}; create={create:?}; failed_attempt={saw_failed_attempt}",
            started.elapsed()
        ))??;
        let create =
            create.ok_or_else(|| "SETUP: capable client received no create".to_string())?;
        if !saw_failed_attempt {
            return Err(
                "refresh answered without observing its real terminal analysis outcome".into(),
            );
        }
        eprintln!(
            "withheld create: real refresh finished in {:?}",
            started.elapsed()
        );
        if reply_late {
            write_lsp_message(
                &mut writer,
                serde_json::json!({
                    "jsonrpc": "2.0", "id": create["id"], "result": null
                }),
            )
            .await?;
            // Keep the session alive after the late response, so a detached
            // announce has an opportunity to run before shutdown/exit.
            let quiet = tokio::time::sleep(Duration::from_millis(100));
            tokio::pin!(quiet);
            loop {
                tokio::select! {
                    () = &mut quiet => break,
                    message = read_lsp_message(&mut reader) => {
                        let message = message?;
                        if message["method"] == "$/progress" {
                            return Err(format!("late create revived terminated token: {message}"));
                        }
                    }
                }
            }
        }
        write_lsp_message(
            &mut writer,
            serde_json::json!({
                "jsonrpc": "2.0", "id": 3, "method": "shutdown", "params": null
            }),
        )
        .await?;
        loop {
            let message = read_lsp_message(&mut reader).await?;
            if message["method"] == "$/progress" {
                return Err(format!(
                    "terminated unstarted token emitted progress: {message}"
                ));
            }
            if message["id"] == 3 && message.get("method").is_none() {
                if message.get("error").is_some() {
                    return Err(format!("shutdown failed: {message}"));
                }
                break;
            }
        }
        write_lsp_message(
            &mut writer,
            serde_json::json!({
                "jsonrpc": "2.0", "method": "exit", "params": null
            }),
        )
        .await?;
        Ok(())
    };
    let result = tokio::time::timeout(Duration::from_secs(10), exchange)
        .await
        .map_err(|_elapsed| "withheld-create exchange blocked shutdown".to_string())
        .and_then(|result| result);
    if let Err(error) = result {
        server.abort();
        let _ = server.await;
        return Err(error);
    }
    let mut server = server;
    match tokio::time::timeout(Duration::from_secs(2), &mut server).await {
        Ok(result) => result.map_err(|error| format!("server task failed: {error}")),
        Err(_) => {
            server.abort();
            let _ = server.await;
            Err("unanswered create kept server alive after shutdown/exit".into())
        }
    }
}

#[test]
#[serial]
fn unanswered_create_does_not_delay_real_refresh_or_shutdown() -> Result<(), String> {
    work_done_progress_runtime()?.block_on(withheld_create_exchange(false))
}

#[test]
#[serial]
fn late_create_success_cannot_begin_a_finished_real_refresh() -> Result<(), String> {
    work_done_progress_runtime()?.block_on(withheld_create_exchange(true))
}
