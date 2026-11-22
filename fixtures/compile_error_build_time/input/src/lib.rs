#[cfg(all(target_os = "emscripten", feature = "process"))]
compile_error!("Feature process is not supported on emscripten.");
#[cfg(all(target_os = "wasi", feature = "signal"))]
compile_error!(
    "Feature signal is not supported on wasi."
);
pub fn limit(n: usize) -> usize {
    n.min(8)
}
