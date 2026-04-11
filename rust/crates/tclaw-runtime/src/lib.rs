use tclaw_api::{canonical_surfaces, SurfaceDescriptor};

pub struct RuntimeBootstrap {
    pub canonical_runtime: &'static str,
    pub surfaces: Vec<SurfaceDescriptor>,
}

impl RuntimeBootstrap {
    pub fn new() -> Self {
        Self {
            canonical_runtime: "rust",
            surfaces: canonical_surfaces(),
        }
    }
}

impl Default for RuntimeBootstrap {
    fn default() -> Self {
        Self::new()
    }
}
