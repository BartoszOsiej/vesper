//! VESPER — nasze EVE. Etap 1: statek z generatora MTS-Ships, lot i pole asteroidów.

mod gfx;
mod market;
mod mesh;

use glam::{Mat4, Vec3};
use gfx::{Instance, Renderer};
use mesh::MeshData;
use std::collections::HashSet;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

/// Statek gracza: pozycja, kąty, prędkość.
struct Ship {
    pos: Vec3,
    yaw: f32,
    pitch: f32,
    roll: f32,
    speed: f32,
}

impl Ship {
    fn forward(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(cy * cp, sp, sy * cp)
    }

    fn rotate(&self) -> Mat4 {
        Mat4::from_rotation_y(self.yaw)
            * Mat4::from_rotation_z(-self.pitch)
            * Mat4::from_rotation_x(self.roll)
    }

    fn model(&self) -> Mat4 {
        Mat4::from_translation(self.pos) * self.rotate()
    }
}

struct Rock {
    pos: Vec3,
    radius: f32,
    mesh: usize,
    left: f32, // pozostała ruda
}

struct State {
    window: Arc<Window>,
    gfx: Renderer,
    ship: Ship,
    ship_mesh: usize,
    rocks: Vec<Rock>,
    target: Option<usize>,
    cargo: f32,
    laser: f32,
    keys: HashSet<KeyCode>,
    last: std::time::Instant,
    spin: f32,
}

impl State {
    async fn new(window: Arc<Window>) -> Self {
        let mut gfx = Renderer::new(window.clone()).await;

        // Handshake marketu: jeśli asset nie zgadza się z manifestem, klient nie startuje.
        let root = std::path::Path::new("assets");
        let market = market::handshake(root, &root.join("market/market.txt"))
            .unwrap_or_else(|e| panic!("rynek odrzucony: {e}"));
        println!("rynek {} v{} — {} pozycji, assety zgodne", market.station, market.version, market.items.len());
        for it in &market.items {
            println!("  {:14} {:10} kupno {:>4}  sprzedaz {:>4}", it.id, it.name, it.buy, it.sell);
        }

        let obj = std::fs::read_to_string("assets/ships/frigate.obj")
            .expect("brak assets/ships/frigate.obj — skopiuj z midpoly/out/ships");
        let ship_data = MeshData::from_obj(&obj).expect("nie udało się wczytać OBJ");
        println!("statek: {} wierzchołków, {} indeksów", ship_data.verts.len(), ship_data.indices.len());
        let ship_mesh = gfx.add_mesh(&ship_data);

        // pole asteroidów wokół startu
        let mut rocks = Vec::new();
        let mut seed = 1u32;
        for i in 0..140 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let r = 40.0 + (seed % 500) as f32;
            let a = (seed % 6283) as f32 / 1000.0;
            let b = ((seed >> 8) % 6283) as f32 / 1000.0;
            let pos = Vec3::new(
                700.0 + a.cos() * r * 6.0,
                (b.sin()) * 220.0,
                b.cos() * r * 6.0,
            );
            let radius = 8.0 + (seed >> 16) as f32 % 55.0;
            let data = MeshData::asteroid(seed, radius, 0.45);
            let mi = gfx.add_mesh(&data);
            rocks.push(Rock { pos, radius, mesh: mi, left: radius * 4.0 });
            let _ = i;
        }

        Self {
            window,
            gfx,
            ship: Ship { pos: Vec3::ZERO, yaw: 0.0, pitch: 0.0, roll: 0.0, speed: 40.0 },
            ship_mesh,
            rocks,
            target: None,
            cargo: 0.0,
            laser: 0.0,
            keys: HashSet::new(),
            last: std::time::Instant::now(),
            spin: 0.0,
        }
    }

    fn update(&mut self) {
        let now = std::time::Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.05);
        self.last = now;

        let k = &self.keys;
        let turn = 1.2 * dt;
        if k.contains(&KeyCode::KeyA) { self.ship.yaw += turn; }
        if k.contains(&KeyCode::KeyD) { self.ship.yaw -= turn; }
        if k.contains(&KeyCode::KeyW) { self.ship.pitch += turn * 0.8; }
        if k.contains(&KeyCode::KeyS) { self.ship.pitch -= turn * 0.8; }
        if k.contains(&KeyCode::KeyQ) { self.ship.roll += turn; }
        if k.contains(&KeyCode::KeyE) { self.ship.roll -= turn; }
        self.ship.pitch = self.ship.pitch.clamp(-1.45, 1.45);

        let boost = if k.contains(&KeyCode::ShiftLeft) { 4.0 } else { 1.0 };
        if k.contains(&KeyCode::ArrowUp) { self.ship.speed = (self.ship.speed + 60.0 * dt * boost).min(900.0); }
        if k.contains(&KeyCode::ArrowDown) { self.ship.speed = (self.ship.speed - 60.0 * dt).max(0.0); }

        let fwd = self.ship.forward();
        self.ship.pos += fwd * self.ship.speed * dt;
        self.spin += dt;

        // namierzanie: najbliższa asteroida w stożku przed statkiem
        let fwd = self.ship.forward();
        let mut best: Option<(usize, f32)> = None;
        for (i, r) in self.rocks.iter().enumerate() {
            if r.left <= 0.0 {
                continue;
            }
            let d = r.pos - self.ship.pos;
            let dist = d.length();
            if dist > 420.0 {
                continue;
            }
            if d.normalize_or_zero().dot(fwd) < 0.90 {
                continue;
            }
            if best.map(|(_, bd)| dist < bd).unwrap_or(true) {
                best = Some((i, dist));
            }
        }
        self.target = best.map(|(i, _)| i);

        // wydobycie: przytrzymaj F na namierzonej asteroidzie
        let mining = k.contains(&KeyCode::KeyF) && self.target.is_some();
        if mining {
            let i = self.target.unwrap();
            let take = (6.0 * dt).min(self.rocks[i].left);
            self.rocks[i].left -= take;
            self.cargo += take * 0.25;
            self.laser = (self.laser + dt * 6.0).min(1.0);
        } else {
            self.laser = (self.laser - dt * 3.0).max(0.0);
        }
    }

    fn render(&mut self) {
        let eye = self.ship.pos - self.ship.forward() * 58.0 + Vec3::new(0.0, 17.0, 0.0);
        let target = self.ship.pos + self.ship.forward() * 34.0;
        let proj = Mat4::perspective_rh(60f32.to_radians(), self.gfx.aspect(), 0.5, 12000.0);
        let view = Mat4::look_at_rh(eye, target, Vec3::Y);
        let vp = proj * view;

        let ship_inst = vec![Instance::new(self.ship.model(), [0.62, 0.68, 0.78, 1.0])];
        let mut rock_map: std::collections::HashMap<usize, Vec<Instance>> = Default::default();
        for (i, r) in self.rocks.iter().enumerate() {
            if r.left <= 0.0 {
                continue;
            }
            let rot = Mat4::from_rotation_y(self.spin * 0.1 + r.radius)
                * Mat4::from_rotation_x(self.spin * 0.07 + r.pos.x);
            let hot = if self.target == Some(i) && self.laser > 0.0 {
                [0.95, 0.55, 0.28, 1.0]
            } else if self.target == Some(i) {
                [0.62, 0.55, 0.44, 1.0]
            } else {
                [0.42, 0.40, 0.37, 1.0]
            };
            rock_map
                .entry(r.mesh)
                .or_default()
                .push(Instance::new(Mat4::from_translation(r.pos) * rot, hot));
        }
        let mut batches = vec![(self.ship_mesh, ship_inst)];
        for (mi, list) in rock_map {
            batches.push((mi, list));
        }
        self.gfx.render(vp, eye, &batches);

        let speed = self.ship.speed;
        let lock = self
            .target
            .map(|i| format!("CEL: asteroida #{i} r={:.0} m", self.rocks[i].radius))
            .unwrap_or_else(|| "CEL: brak".into());
        self.window.set_title(&format!(
            "VESPER — {speed:.0} m/s  |  ruda: {:.1} t  |  {lock}  |  [WSAD] kierunek [QE] roll [↑↓] ciąg [Shift] dopalacz [F] wydobycie",
            self.cargo
        ));
    }
}

struct App {
    state: Option<State>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("VESPER")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0));
        let window = Arc::new(event_loop.create_window(attrs).expect("brak okna"));
        self.state = Some(pollster::block_on(State::new(window)));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(state) = self.state.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => state.gfx.resize(size.width, size.height),
            WindowEvent::KeyboardInput { event: KeyEvent { physical_key: PhysicalKey::Code(code), state: ks, .. }, .. } => {
                match ks {
                    ElementState::Pressed => { state.keys.insert(code); }
                    ElementState::Released => { state.keys.remove(&code); }
                }
                if code == KeyCode::Escape && ks == ElementState::Pressed {
                    event_loop.exit();
                }
            }
            WindowEvent::RedrawRequested => {
                state.update();
                state.render();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("brak pętli zdarzeń");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App { state: None };
    event_loop.run_app(&mut app).expect("pętla padła");
}
