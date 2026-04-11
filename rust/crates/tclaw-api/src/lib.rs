use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SurfaceDescriptor {
    pub name: &'static str,
    pub role: &'static str,
}

pub fn canonical_surfaces() -> Vec<SurfaceDescriptor> {
    vec![
        SurfaceDescriptor {
            name: "cli",
            role: "operator entrypoint",
        },
        SurfaceDescriptor {
            name: "runtime",
            role: "canonical daemon implementation",
        },
        SurfaceDescriptor {
            name: "tools",
            role: "tool integration boundary",
        },
        SurfaceDescriptor {
            name: "plugins",
            role: "plugin integration boundary",
        },
    ]
}
