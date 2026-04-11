use tclaw_runtime::{runtime_command_registry, RuntimeBootstrap};

fn main() {
    let runtime = RuntimeBootstrap::new();
    let registry = runtime_command_registry().expect("command registry should be valid");
    println!(
        "tclaw-cli bootstrap: canonical runtime={}, surfaces={}, builtins={}, plugins={}",
        runtime.canonical_runtime,
        runtime.surfaces.len(),
        registry.built_in_commands().len(),
        registry.plugin_commands().len()
    );
}
