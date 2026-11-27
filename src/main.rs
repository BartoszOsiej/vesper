//! VESPER — nasze EVE. Lot, wydobycie, stacja i handel.

mod gfx;
mod market;
mod mesh;
mod ui;

use glam::{Mat4, Vec3};
use gfx::{Instance, Renderer};
use mesh::MeshData;
use std::collections::HashSet;
use std::sync::Arc;
use ui::UiRect;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::{Window, WindowId};

const CREAM: [f32; 4] = [0.85, 0.88, 0.92, 1.0];
const DIM: [f32; 4] = [0.45, 0.50, 0.58, 1.0];
const ACCENT: [f32; 4] = [0.35, 0.85, 0.95, 1.0];
const WARN: [f32; 4] = [0.95, 0.62, 0.30, 1.0];

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

    fn model(&self) -> Mat4 {
        Mat4::from_translation(self.pos)
            * Mat4::from_rotation_y(self.yaw)
            * Mat4::from_rotation_z(-self.pitch)
            * Mat4::from_rotation_x(self.roll)
    }
}

struct Rock {
    pos: Vec3,
    radius: f32,
    mesh: usize,
    left: f32,
}

struct State {
    window: Arc<Window>,
    gfx: Renderer,
    ship: Ship,
    ship_mesh: usize,
    station_mesh: usize,
    station_pos: Vec3,
    market: market::Market,
    rocks: Vec<Rock>,
    target: Option<usize>,
    cargo: f32,
    credits: u64,
    fuel: u32,
    laser: f32,
    docked: bool,
    log: String,
    keys: HashSet<KeyCode>,
    shift: bool,
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
        println!("rynek {} v{} — assety zgodne", market.station, market.version);
        for it in &market.items {
            println!("  {:14} {:10} kupno {:>4}  sprzedaz {:>4}", it.id, it.name, it.buy, it.sell);
        }

        let obj = std::fs::read_to_string("assets/ships/frigate.obj")
            .expect("brak assets/ships/frigate.obj — skopiuj z midpoly/out/ships");
        let ship_data = MeshData::from_obj(&obj).expect("nie udało się wczytać OBJ");
        println!("statek: {} wierzchołków, {} indeksów", ship_data.verts.len(), ship_data.indices.len());
        let ship_mesh = gfx.add_mesh(&ship_data);
        let station_mesh = gfx.add_mesh(&MeshData::station(120.0));

        let mut rocks = Vec::new();
        let mut seed = 1u32;
        for _ in 0..140 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let r = 40.0 + (seed % 500) as f32;
            let a = (seed % 6283) as f32 / 1000.0;
            let b = ((seed >> 8) % 6283) as f32 / 1000.0;
            let pos = Vec3::new(
                700.0 + a.cos() * r * 6.0,
                b.sin() * 220.0,
                b.cos() * r * 6.0,
            );
            let radius = 8.0 + (seed >> 16) as f32 % 55.0;
            let mi = gfx.add_mesh(&MeshData::asteroid(seed, radius, 0.45));
            rocks.push(Rock { pos, radius, mesh: mi, left: radius * 4.0 });
        }

        Self {
            window,
            gfx,
            ship: Ship { pos: Vec3::new(0.0, 0.0, -500.0), yaw: -std::f32::consts::FRAC_PI_2, pitch: 0.0, roll: 0.0, speed: 40.0 },
            ship_mesh,
            station_mesh,
            station_pos: Vec3::new(0.0, 0.0, -1400.0),
            market,
            rocks,
            target: None,
            cargo: 0.0,
            credits: 500,
            fuel: 10,
            laser: 0.0,
            docked: false,
            log: "Witaj w VESPER".into(),
            keys: HashSet::new(),
            shift: false,
            last: std::time::Instant::now(),
            spin: 0.0,
        }
        .with_debug_flags()
    }

    /// Flagi startowe do testów: VESPER_START_DOCKED=1 otwiera rynek od razu.
    fn with_debug_flags(mut self) -> Self {
        if std::env::var("VESPER_START_DOCKED").is_ok() {
            self.docked = true;
            self.ship.speed = 0.0;
        }
        self
    }

    fn update(&mut self) {
        let now = std::time::Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.05);
        self.last = now;
        self.spin += dt;
        if self.docked {
            return;
        }

        let k = self.keys.clone();
        let turn = 1.2 * dt;
        if k.contains(&KeyCode::KeyA) { self.ship.yaw += turn; }
        if k.contains(&KeyCode::KeyD) { self.ship.yaw -= turn; }
        if k.contains(&KeyCode::KeyW) { self.ship.pitch += turn * 0.8; }
        if k.contains(&KeyCode::KeyS) { self.ship.pitch -= turn * 0.8; }
        if k.contains(&KeyCode::KeyQ) { self.ship.roll += turn; }
        if k.contains(&KeyCode::KeyE) { self.ship.roll -= turn; }
        self.ship.pitch = self.ship.pitch.clamp(-1.45, 1.45);

        let boost = if k.contains(&KeyCode::ShiftLeft) { 4.0 } else { 1.0 };
        if k.contains(&KeyCode::ArrowUp) {
            self.ship.speed = (self.ship.speed + 60.0 * dt * boost).min(900.0);
        }
        if k.contains(&KeyCode::ArrowDown) {
            self.ship.speed = (self.ship.speed - 60.0 * dt).max(0.0);
        }

        self.ship.pos += self.ship.forward() * self.ship.speed * dt;

        // namierzanie
        let fwd = self.ship.forward();
        let mut best: Option<(usize, f32)> = None;
        for (i, r) in self.rocks.iter().enumerate() {
            if r.left <= 0.0 {
                continue;
            }
            let d = r.pos - self.ship.pos;
            let dist = d.length();
            if dist > 420.0 || d.normalize_or_zero().dot(fwd) < 0.90 {
                continue;
            }
            if best.map(|(_, bd)| dist < bd).unwrap_or(true) {
                best = Some((i, dist));
            }
        }
        self.target = best.map(|(i, _)| i);

        // wydobycie
        if k.contains(&KeyCode::KeyF) {
            if let Some(i) = self.target {
                let take = (6.0 * dt).min(self.rocks[i].left);
                self.rocks[i].left -= take;
                self.cargo += take * 0.25;
                self.laser = (self.laser + dt * 6.0).min(1.0);
            }
        } else {
            self.laser = (self.laser - dt * 3.0).max(0.0);
        }
    }

    fn dock(&mut self) {
        if self.ship.pos.distance(self.station_pos) < 700.0 {
            self.docked = true;
            self.ship.speed = 0.0;
            self.log = "Dokowanie zakończone".into();
        } else {
            self.log = "Za daleko od stacji".into();
        }
    }

    fn trade(&mut self, index: usize, sell: bool) {
        let Some(item) = self.market.items.get(index).cloned() else { return };
        if sell {
            let amount = (self.cargo.floor() as u64).min(200);
            if amount == 0 {
                self.log = "Brak rudy w ładowni".into();
                return;
            }
            let gain = amount * item.sell;
            self.cargo -= amount as f32;
            self.credits += gain;
            self.log = format!("Sprzedano {amount} x {} za {gain} kr", item.name);
        } else {
            let cost = item.buy;
            if self.credits < cost {
                self.log = "Za mało kredytów".into();
                return;
            }
            if item.id == "fuel" {
                self.fuel += 1;
            } else {
                self.cargo += 5.0;
            }
            self.credits -= cost;
            self.log = format!("Kupiono {} za {cost} kr", item.name);
        }
    }

    fn hud(&self) -> Vec<UiRect> {
        let mut r = Vec::new();
        let h = self.gfx.height() as f32;

        if self.docked {
            // panel
            r.push(UiRect::new(0.0, 0.0, self.gfx.width() as f32, h, [0.0, 0.0, 0.0, 0.55]));
            let px = 3.0;
            let panel_x = 80.0;
            let mut y = 70.0;
            r.push(UiRect::new(panel_x, y - 20.0, 760.0, 4.0, ACCENT));
            y += 12.0;
            ui::text(&mut r, panel_x, y, &format!("STACJA {}", self.market.station), px * 2.0, ACCENT);
            y += 40.0;
            ui::text(&mut r, panel_x, y, "RYNEK — 1 SPRZEDAJ RUDE  2 KUP PALIWO  3 KUP STOP  G ODKACZ", px, CREAM);
            y += 34.0;
            for (i, it) in self.market.items.iter().enumerate() {
                let line = format!("{}  {}   KUPNO {}  SPRZEDAZ {}", i + 1, it.name, it.buy, it.sell);
                ui::text(&mut r, panel_x, y, &line, px, if i == 0 { ACCENT } else { CREAM });
                y += 24.0;
            }
            y += 16.0;
            ui::text(&mut r, panel_x, y, &format!("KREDYTY {}   RUD  {:.1}   PALIWO {}", self.credits, self.cargo, self.fuel), px, CREAM);
            y += 30.0;
            ui::text(&mut r, panel_x, y, &self.log, px, WARN);
            return r;
        }

        // HUD w locie
        let px = 2.6;
        ui::text(&mut r, 24.0, h - 88.0, &format!("{:.0} M/S", self.ship.speed), px, ACCENT);
        ui::text(&mut r, 24.0, h - 60.0, &format!("KREDYTY {}  RUD {:.1}  PALIWO {}", self.credits, self.cargo, self.fuel), px, CREAM);

        let lock = match self.target {
            Some(i) => format!("CEL ASTEROIDA {} R {:.0}", i, self.rocks[i].radius),
            None => "CEL BRAK".to_string(),
        };
        ui::text(&mut r, 24.0, h - 34.0, &lock, px, if self.target.is_some() { WARN } else { DIM });

        let dist = self.ship.pos.distance(self.station_pos);
        if dist < 900.0 {
            ui::text(&mut r, 24.0, 40.0, &format!("STACJA {:.0} M — G DOKUJ", dist), px, ACCENT);
        }
        if !self.log.is_empty() {
            ui::text(&mut r, 24.0, 70.0, &self.log, px * 0.9, DIM);
        }
        r
    }

    fn render(&mut self) {
        let (eye, target) = if self.docked {
            let around = self.station_pos + Vec3::new(230.0, 120.0, 230.0);
            (around, self.station_pos)
        } else {
            (
                self.ship.pos - self.ship.forward() * 58.0 + Vec3::new(0.0, 17.0, 0.0),
                self.ship.pos + self.ship.forward() * 34.0,
            )
        };
        let proj = Mat4::perspective_rh(60f32.to_radians(), self.gfx.aspect(), 0.5, 20000.0);
        let view = Mat4::look_at_rh(eye, target, Vec3::Y);
        let vp = proj * view;

        let ship_inst = vec![Instance::new(self.ship.model(), [0.62, 0.68, 0.78, 1.0])];
        let station_inst = vec![Instance::new(
            Mat4::from_translation(self.station_pos) * Mat4::from_rotation_y(self.spin * 0.05),
            [0.30, 0.33, 0.38, 1.0],
        )];

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
            rock_map.entry(r.mesh).or_default().push(Instance::new(Mat4::from_translation(r.pos) * rot, hot));
        }

        let mut batches = vec![(self.ship_mesh, ship_inst), (self.station_mesh, station_inst)];
        for (mi, list) in rock_map {
            batches.push((mi, list));
        }

        let ui = self.hud();
        self.gfx.render(vp, eye, &batches, &ui);

        self.window.set_title(&format!(
            "VESPER — {}  |  kredyty {}  ruda {:.1}",
            if self.docked { "DOK" } else { "LOT" },
            self.credits,
            self.cargo
        ));
    }

    fn on_key(&mut self, code: KeyCode, pressed: bool) {
        if pressed {
            self.keys.insert(code);
            match code {
                KeyCode::KeyG => {
                    if self.docked {
                        self.docked = false;
                        self.ship.speed = 40.0;
                        self.log = "Odczepiono — lot swobodny".into();
                    } else {
                        self.dock();
                    }
                }
                KeyCode::Digit1 if self.docked => self.trade(0, !self.shift),
                KeyCode::Digit2 if self.docked => self.trade(1, !self.shift),
                KeyCode::Digit3 if self.docked => self.trade(2, !self.shift),
                _ => {}
            }
        } else {
            self.keys.remove(&code);
        }
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
            WindowEvent::ModifiersChanged(m) => {
                let s: ModifiersState = m.state();
                state.shift = s.shift_key();
            }
            WindowEvent::Resized(size) => state.gfx.resize(size.width, size.height),
            WindowEvent::KeyboardInput {
                event: KeyEvent { physical_key: PhysicalKey::Code(code), state: ks, .. },
                ..
            } => {
                if code == KeyCode::Escape && ks == ElementState::Pressed {
                    event_loop.exit();
                }
                state.on_key(code, ks == ElementState::Pressed);
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
