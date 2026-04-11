use tclaw_runtime::RuntimeBootstrap;

fn main() {
    let runtime = RuntimeBootstrap::new();
    println!(
        "tclaw-cli bootstrap: canonical runtime={}, surfaces={}",
        runtime.canonical_runtime,
        runtime.surfaces.len()
    );
}
