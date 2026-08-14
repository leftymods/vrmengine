use glam::{Mat4, Vec3};

/// A simple orbit camera around a target point.
#[derive(Clone, Copy)]
pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
    pub target: Vec3,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            yaw: 0.7,
            pitch: 0.25,
            dist: 3.0,
            target: Vec3::new(0.0, 1.0, 0.0),
        }
    }
}

impl Camera {
    /// Center the camera on the given bounding box.
    pub fn frame(&mut self, min: Vec3, max: Vec3) {
        self.target = (min + max) * 0.5;
        let radius = (max - min).length() * 0.5;
        self.dist = (radius * 2.6).max(0.5);
    }

    pub fn eye(&self) -> Vec3 {
        let cp = self.pitch.cos();
        self.target
            + self.dist * Vec3::new(self.yaw.cos() * cp, self.pitch.sin(), self.yaw.sin() * cp)
    }

    pub fn view(&self) -> Mat4 {
        Mat4::look_at_rh(self.eye(), self.target, Vec3::Y)
    }

    pub fn proj(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(45f32.to_radians(), aspect.max(0.001), 0.01, 200.0)
    }
}
