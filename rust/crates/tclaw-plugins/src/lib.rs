use tclaw_api::SurfaceDescriptor;

pub fn plugin_surface() -> SurfaceDescriptor {
    SurfaceDescriptor {
        name: "plugins",
        role: "plugin loading boundary",
    }
}
