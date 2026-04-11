use tclaw_api::SurfaceDescriptor;

pub fn tool_surface() -> SurfaceDescriptor {
    SurfaceDescriptor {
        name: "tools",
        role: "tool adapter boundary",
    }
}
