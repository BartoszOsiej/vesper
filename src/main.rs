//! VESPER — nasze EVE. Lot, walka, wydobycie, stacja, handel, dwa sektory, zapis.

mod gfx;
mod market;
mod mesh;
mod textures;
mod ui;

use glam::{Mat4, Quat, Vec3};
use gfx::{Instance, Renderer};
use mesh::MeshData;
use std::collections::HashSet;
use std::sync::Arc;
use ui::UiRect;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

const CREAM: [f32; 4] = [0.85, 0.88, 0.92, 1.0];
const DIM: [f32; 4] = [0.45, 0.50, 0.58, 1.0];
const ACCENT: [f32; 4] = [0.35, 0.85, 0.95, 1.0];
const WARN: [f32; 4] = [0.95, 0.62, 0.30, 1.0];
const DANGER: [f32; 4] = [0.95, 0.35, 0.32, 1.0];

/// Pojemność ładowni w m³. Ruda zajmuje 1 m³, stop 0,6 m³ na jednostkę.
const CARGO_CAP: f32 = 200.0;
const ALLOY_VOL: f32 = 0.6;
/// Jedna partia rafinerii: 10 rudy daje 8 stopu.
const REFINE_IN: f32 = 10.0;
const REFINE_OUT: f32 = 8.0;
/// Stacja jest 2,2x większa od siatki bazowej — skala kontra 35-metrowy statek.
const STATION_SCALE: f32 = 2.2;

#[derive(Debug, PartialEq)]
enum RefineError {
    NoOre,
    NoSpace,
}

/// Lokalna różnica cen wobec bazy z `market.txt`. HARTWELL to hub przemysłowy
/// (tanie paliwo, tani stop), KEPLER-9 to frontier (drogi stop i paliwo).
fn sector_price_delta(sector: usize, id: &str) -> i64 {
    match (sector % 2, id) {
        (0, "ore_raw") => 2,
        (1, "ore_raw") => -2,
        (0, "ore_refined") => -4,
        (1, "ore_refined") => 6,
        (0, "fuel") => -10,
        (1, "fuel") => 15,
        _ => 0,
    }
}

fn station_price(base: u64, sector: usize, id: &str) -> u64 {
    (base as i64 + sector_price_delta(sector, id)).max(1) as u64
}

/// Macierz sprite'a zwróconego w stronę kamery.
fn billboard(pos: Vec3, right: Vec3, up: Vec3, fwd: Vec3, sx: f32, sy: f32) -> Mat4 {
    Mat4::from_cols(
        (right * sx).extend(0.0),
        (up * sy).extend(0.0),
        (fwd * sx.min(sy)).extend(0.0),
        pos.extend(1.0),
    )
}

fn hud_bar(out: &mut Vec<UiRect>, x: f32, y: f32, w: f32, h: f32, frac: f32, color: [f32; 4]) {
    out.push(UiRect::new(x, y, w, h, [0.05, 0.065, 0.09, 0.82]));
    let f = frac.clamp(0.0, 1.0);
    if f > 0.0 {
        out.push(UiRect::new(x, y, w * f, h, color));
    }
    out.push(UiRect::new(x, y, w, 1.0, [1.0, 1.0, 1.0, 0.10]));
}

fn right_text(out: &mut Vec<UiRect>, right: f32, y: f32, s: &str, px: f32, c: [f32; 4]) {
    let x = right - ui::text_width(s, px);
    ui::text(out, x, y, s, px, c);
}

/// Czysta logika rafinerii — bez stanu gry, żeby dało się ją przetestować.
fn refine_batch(ore: f32, alloy: f32) -> Result<(f32, f32), RefineError> {
    if ore < REFINE_IN {
        return Err(RefineError::NoOre);
    }
    let new_ore = ore - REFINE_IN;
    let new_alloy = alloy + REFINE_OUT;
    if new_ore + new_alloy * ALLOY_VOL > CARGO_CAP + 0.001 {
        return Err(RefineError::NoSpace);
    }
    Ok((new_ore, new_alloy))
}

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
            * Mat4::from_scale(Vec3::splat(1.35))
    }
}

struct Rock {
    pos: Vec3,
    radius: f32,
    mesh: usize,
    left: f32,
}

struct Enemy {
    pos: Vec3,
    hp: f32,
    cooldown: f32,
    drift: f32,
}

struct Particle {
    pos: Vec3,
    vel: Vec3,
    life: f32,
    max: f32,
    size: f32,
    color: [f32; 3],
}

struct Beam {
    a: Vec3,
    b: Vec3,
    life: f32,
    max: f32,
    color: [f32; 3],
    width: f32,
}

struct RockSpawn {
    pos: Vec3,
    radius: f32,
    mesh: usize,
}

struct SectorLayout {
    name: &'static str,
    station: Vec3,
    tint: [f32; 3],
    rocks: Vec<RockSpawn>,
    enemies: Vec<Vec3>,
}

struct State {
    window: Arc<Window>,
    gfx: Renderer,
    ship: Ship,
    ship_mesh: usize,
    station_mesh: usize,
    cube_mesh: usize,
    sectors: Vec<SectorLayout>,
    sector: usize,
    station_pos: Vec3,
    station_tint: [f32; 3],
    market: market::Market,
    rocks: Vec<Rock>,
    target: Option<usize>,
    /// Ruda w ładowni (1 m³/szt.).
    cargo: f32,
    /// Stop z rafinerii (0,6 m³/szt.).
    alloy: f32,
    credits: u64,
    fuel: u32,
    laser: f32,
    docked: bool,
    enemies: Vec<Enemy>,
    hull: f32,
    shield: f32,
    hit_flash: f32,
    kills: u32,
    contract_goal: u32,
    contract_done: u32,
    contract_reward: u64,
    particles: Vec<Particle>,
    beams: Vec<Beam>,
    stars: Vec<(Vec3, f32, f32, [f32; 3])>,
    quad_mesh: usize,
    cam_pos: Vec3,
    cam_target: Vec3,
    cam_up: Vec3,
    cam_fov: f32,
    emit: f32,
    rng: u32,
    flash: f32,
    respawn: f32,
    log: String,
    keys: HashSet<KeyCode>,
    shift: bool,
    fullscreen: bool,
    last: std::time::Instant,
    spin: f32,
}

impl State {
    async fn new(window: Arc<Window>) -> Self {
        let mut gfx = Renderer::new(window.clone()).await;

        // Handshake marketu: jeśli asset nie zgadza się z manifestem, klient nie startuje.
        let root = std::path::Path::new("assets");
        let market = market::handshake(root, &root.join("market/market.txt"))
            .unwrap_or_else(|e| panic!("market rejected: {e}"));
        println!("market {} v{} — assets verified", market.station, market.version);
        for it in &market.items {
            println!("  {:14} {:10} buy {:>4}  sell {:>4}", it.id, it.name, it.buy, it.sell);
        }

        // Kadłub jest generowany proceduralnie; plik OBJ zostaje w manifeście
        // rynku jako asset referencyjny generatora MTS-Ships.
        let ship_data = MeshData::frigate();
        println!("ship: {} vertices, {} indices", ship_data.verts.len(), ship_data.indices.len());
        let ship_mesh = gfx.add_mesh(&ship_data);
        let station_mesh = gfx.add_mesh(&MeshData::station(120.0));
        let cube_mesh = gfx.add_mesh(&MeshData::cube(1.0));
        let quad_mesh = gfx.add_mesh(&MeshData::quad());

        let sectors = vec![
            build_sector(&mut gfx, "HARTWELL", Vec3::new(0.0, 0.0, -1400.0), [0.30, 0.33, 0.38], 1, 140, 6),
            build_sector(&mut gfx, "KEPLER-9", Vec3::new(0.0, 0.0, -2200.0), [0.34, 0.29, 0.26], 7, 210, 9),
        ];

        // gwiazdy: trzy warstwy jasności i temperatury barwnej, jako sprite'y
        let mut stars = Vec::new();
        let mut sseed = 12345u32;
        let mut srand = || {
            sseed = sseed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (sseed >> 8) as f32 / 16_777_216.0
        };
        for _ in 0..1500 {
            let u = srand();
            let v = srand();
            let t = srand();
            let br = srand();
            let a = u * std::f32::consts::TAU;
            let lat = (v * 2.0 - 1.0).asin();
            let dir = Vec3::new(lat.cos() * a.cos(), lat.sin(), lat.cos() * a.sin());
            let mut size = 12.0 + br * br * 80.0;
            let bright = 0.22 + 0.78 * br * br;
            if br > 0.985 {
                size *= 2.4;
            }
            let color = if t < 0.18 {
                [1.0, 0.76, 0.52]
            } else if t < 0.40 {
                [0.70, 0.82, 1.0]
            } else if t < 0.62 {
                [0.92, 0.95, 1.0]
            } else {
                [1.0, 0.93, 0.84]
            };
            stars.push((dir * 12_000.0, size, bright, color));
        }

        let mut st = Self {
            window,
            gfx,
            ship: Ship { pos: Vec3::new(0.0, 0.0, -500.0), yaw: -std::f32::consts::FRAC_PI_2, pitch: 0.0, roll: 0.0, speed: 40.0 },
            ship_mesh,
            station_mesh,
            cube_mesh,
            sectors,
            sector: 0,
            station_pos: Vec3::ZERO,
            station_tint: [0.30, 0.33, 0.38],
            market,
            rocks: Vec::new(),
            target: None,
            cargo: 0.0,
            alloy: 0.0,
            credits: 500,
            fuel: 10,
            laser: 0.0,
            docked: false,
            enemies: Vec::new(),
            hull: 100.0,
            shield: 60.0,
            hit_flash: 0.0,
            kills: 0,
            contract_goal: 3,
            contract_done: 0,
            contract_reward: 900,
            particles: Vec::new(),
            beams: Vec::new(),
            stars,
            quad_mesh,
            cam_pos: Vec3::ZERO,
            cam_target: Vec3::ZERO,
            cam_up: Vec3::Y,
            cam_fov: 55.0,
            emit: 0.0,
            rng: 0x5EED_1234,
            flash: 0.0,
            respawn: 20.0,
            log: "Welcome to VESPER — hostiles in the sector".into(),
            keys: HashSet::new(),
            shift: false,
            fullscreen: false,
            last: std::time::Instant::now(),
            spin: 0.0,
        };
        st.enter_sector(0);
        st.with_debug_flags()
    }

    /// Flagi startowe do testów: VESPER_START_DOCKED=1 otwiera rynek od razu.
    fn with_debug_flags(mut self) -> Self {
        if std::env::var("VESPER_START_DOCKED").is_ok() {
            self.docked = true;
            self.ship.speed = 0.0;
        }
        if let Ok(v) = std::env::var("VESPER_SECTOR") {
            if let Ok(i) = v.parse::<usize>() {
                self.enter_sector(i % self.sectors.len());
            }
        }
        self
    }

    fn rnd(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.rng >> 8) as f32 / 16_777_216.0
    }

    fn enter_sector(&mut self, idx: usize) {
        let layout = &self.sectors[idx];
        self.sector = idx;
        self.station_pos = layout.station;
        self.station_tint = layout.tint;
        self.ship.pos = layout.station + Vec3::new(200.0, 110.0, 760.0);
        self.ship.yaw = -std::f32::consts::FRAC_PI_2;
        self.ship.pitch = 0.0;
        self.ship.roll = 0.0;
        self.ship.speed = 40.0;

        // kamera startuje od razu za statkiem, bez przelotu z poprzedniego sektora
        let fwd = self.ship.forward();
        self.cam_pos = self.ship.pos - fwd * 146.0 + Vec3::Y * 30.0;
        self.cam_target = self.ship.pos + fwd * 230.0 + Vec3::Y * 6.0;
        self.cam_up = Vec3::Y;
        self.cam_fov = 55.0;

        // asteroidy nie mogą przecinać stacji ani punktu startu
        self.rocks = layout
            .rocks
            .iter()
            .filter(|r| {
                r.pos.distance(layout.station) > r.radius + 320.0
                    && r.pos.distance(self.ship.pos) > r.radius + 260.0
            })
            .map(|r| Rock { pos: r.pos, radius: r.radius, mesh: r.mesh, left: r.radius * 4.0 })
            .collect();
        self.enemies = layout
            .enemies
            .iter()
            .enumerate()
            .map(|(i, p)| Enemy { pos: *p, hp: 100.0, cooldown: 0.6 + i as f32 * 0.2, drift: i as f32 * 1.7 })
            .collect();
        self.particles.clear();
        self.beams.clear();
        self.target = None;
        self.respawn = 20.0;
    }

    fn warp(&mut self) {
        if self.docked {
            self.log = "Najpierw odczep (G)".into();
            return;
        }
        if self.fuel == 0 {
            self.log = "No fuel — buy at the station".into();
            return;
        }
        self.fuel -= 1;
        let next = (self.sector + 1) % self.sectors.len();
        let name = self.sectors[next].name;
        self.enter_sector(next);
        self.flash = 1.0;
        self.log = format!("Jump complete: sector {name}");
    }

    fn explode(&mut self, at: Vec3, n: usize, color: [f32; 3], power: f32) {
        for _ in 0..n {
            let dx = self.rnd() * 2.0 - 1.0;
            let dy = self.rnd() * 2.0 - 1.0;
            let dz = self.rnd() * 2.0 - 1.0;
            let sp = power * (0.25 + self.rnd());
            let life = 0.45 + self.rnd() * 0.55;
            let size = 0.9 + self.rnd() * 2.2;
            let tint = 0.55 + self.rnd() * 0.45;
            self.particles.push(Particle {
                pos: at,
                vel: Vec3::new(dx, dy, dz).normalize_or_zero() * sp,
                life,
                max: life,
                size,
                color: [color[0] * tint, color[1] * tint, color[2] * tint],
            });
        }
    }

    fn update_fx(&mut self, dt: f32) {
        for p in self.particles.iter_mut() {
            p.pos += p.vel * dt;
            p.vel *= 1.0 - (1.6 * dt).min(0.9);
            p.life -= dt;
        }
        self.particles.retain(|p| p.life > 0.0);
        for b in self.beams.iter_mut() {
            b.life -= dt;
        }
        self.beams.retain(|b| b.life > 0.0);

        // smuga z silników
        if self.docked || self.ship.speed < 4.0 {
            return;
        }
        let fwd = self.ship.forward();
        let right = Vec3::new(fwd.z, 0.0, -fwd.x).normalize_or_zero();
        self.emit += dt * (10.0 + self.ship.speed * 0.10);
        let mut spawn: Vec<Particle> = Vec::new();
        while self.emit >= 1.0 {
            self.emit -= 1.0;
            for side in [-1.0f32, 1.0] {
                let j = Vec3::new(self.rnd() - 0.5, self.rnd() - 0.5, self.rnd() - 0.5) * 8.0;
                let pos = self.ship.pos - fwd * 17.0 + right * side * 3.6 + j;
                let life = 0.35 + self.rnd() * 0.35;
                spawn.push(Particle {
                    pos,
                    vel: fwd * (self.ship.speed - 14.0 - self.rnd() * 26.0) - right * side * 2.0,
                    life,
                    max: life,
                    size: 0.35 + self.rnd() * 0.55,
                    color: [0.16, 0.38, 0.85],
                });
            }
        }
        self.particles.extend(spawn);
    }

    /// Kamera z opóźnieniem: statek nie skacze w kadrze, a przy dokowaniu
    /// przechodzi w powolny oblot stacji.
    fn update_camera(&mut self, dt: f32) {
        let (eye, target, up, fov) = if self.docked {
            let a = self.spin * 0.055 + 0.7;
            let eye = self.station_pos
                + Vec3::new(a.cos() * 1010.0, 360.0 + (self.spin * 0.13).sin() * 40.0, a.sin() * 1010.0);
            (eye, self.station_pos + Vec3::new(0.0, 26.0, 0.0), Vec3::Y, 52.0)
        } else {
            let fwd = self.ship.forward();
            let q = Quat::from_rotation_y(self.ship.yaw)
                * Quat::from_rotation_z(-self.ship.pitch)
                * Quat::from_rotation_x(self.ship.roll);
            let bank = q * Vec3::Y;
            let up = (Vec3::Y * 0.78 + bank * 0.22).normalize_or_zero();
            let speed = self.ship.speed;
            let dist = 146.0 + speed * 0.03;
            let right = fwd.cross(up).normalize_or_zero();
            let eye = self.ship.pos - fwd * dist - right * 52.0 + up * (34.0 + speed * 0.003);
            let target = self.ship.pos + fwd * 230.0 + up * 4.0;
            (eye, target, up, 55.0 + speed / 900.0 * 6.0)
        };
        let k = 1.0 - (-dt * 5.0).exp();
        self.cam_pos += (eye - self.cam_pos) * k;
        self.cam_target += (target - self.cam_target) * k;
        self.cam_up = (self.cam_up + (up - self.cam_up) * k).normalize_or_zero();
        self.cam_fov += (fov - self.cam_fov) * k;
    }

    fn update(&mut self) {
        let now = std::time::Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.05);
        self.last = now;
        self.spin += dt;
        self.flash = (self.flash - dt * 2.2).max(0.0);
        self.hit_flash = (self.hit_flash - dt * 2.0).max(0.0);
        self.update_fx(dt);
        self.update_camera(dt);

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

        // asteroidy i stacja to bryły: kolizja wypycha statek i uszkadza osłonę
        let mut hit_rock: Option<(Vec3, f32)> = None;
        for r in &self.rocks {
            if r.left <= 0.0 {
                continue;
            }
            if self.ship.pos.distance(r.pos) < r.radius * 0.85 + 6.0 {
                hit_rock = Some((r.pos, r.radius));
                break;
            }
        }
        if let Some((rp, rad)) = hit_rock {
            let n = (self.ship.pos - rp).normalize_or_zero();
            self.ship.pos = rp + n * (rad * 0.85 + 6.0);
            let impact = self.ship.speed;
            self.ship.speed = (impact * 0.22).max(20.0);
            if impact > 70.0 {
                let dmg = (impact * 0.06).clamp(4.0, 55.0);
                if self.shield > 0.0 {
                    self.shield = (self.shield - dmg).max(0.0);
                } else {
                    self.hull = (self.hull - dmg * 0.7).max(0.0);
                }
                self.hit_flash = 1.0;
                self.explode(self.ship.pos, 6, [1.0, 0.62, 0.3], 34.0);
                self.log = "Hull impact — shields took damage".into();
            }
        }
        let sd = self.ship.pos - self.station_pos;
        if sd.length() < 155.0 * STATION_SCALE {
            let n = sd.normalize_or_zero();
            self.ship.pos = self.station_pos + n * 155.0 * STATION_SCALE;
            self.ship.speed *= 0.3;
            self.hit_flash = 1.0;
            self.log = "Warning: station structure".into();
        }

        // namierzanie asteroidy
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
                if self.cargo + self.alloy * ALLOY_VOL >= CARGO_CAP {
                    self.laser = 0.0;
                    self.log = "Cargo full — sell ore or refine alloy".into();
                } else {
                    let take = (6.0 * dt).min(self.rocks[i].left);
                    self.rocks[i].left -= take;
                    self.cargo += take * 0.25;
                    self.laser = (self.laser + dt * 6.0).min(1.0);
                    if self.rnd() < dt * 22.0 {
                        let at = self.rocks[i].pos + Vec3::new(0.0, self.rocks[i].radius, 0.0);
                        self.explode(at, 2, [1.0, 0.78, 0.35], 26.0);
                    }
                    // ciągła wiązka wydobywcza z dziobu do skały
                    let rp = self.rocks[i].pos;
                    let rad = self.rocks[i].radius;
                    let dir = (self.ship.pos - rp).normalize_or_zero();
                    if self.beams.len() < 24 {
                        self.beams.push(Beam {
                            a: self.ship.pos + fwd * 15.0,
                            b: rp + dir * rad,
                            life: 0.05,
                            max: 0.05,
                            color: [0.25, 0.80, 1.0],
                            width: 0.32,
                        });
                    }
                }
            }
        } else {
            self.laser = (self.laser - dt * 3.0).max(0.0);
        }

        // --- walka ---
        let mut enemy_lock: Option<(usize, f32)> = None;
        for (i, e) in self.enemies.iter().enumerate() {
            let d = e.pos - self.ship.pos;
            let dist = d.length();
            if dist < 700.0 && d.normalize_or_zero().dot(fwd) > 0.88 {
                if enemy_lock.map(|(_, bd)| dist < bd).unwrap_or(true) {
                    enemy_lock = Some((i, dist));
                }
            }
        }
        if k.contains(&KeyCode::Space) {
            if let Some((i, _)) = enemy_lock {
                self.enemies[i].hp -= 34.0 * dt;
                self.laser = (self.laser + dt * 8.0).min(1.0);
                if self.rnd() < dt * 9.0 {
                    let hit = self.enemies[i].pos;
                    self.explode(hit, 2, [1.0, 0.62, 0.28], 30.0);
                }
                // tracer z dziobu do celu
                let from = self.ship.pos + fwd * 14.0;
                let to = self.enemies[i].pos;
                if self.beams.len() < 24 {
                    self.beams.push(Beam { a: from, b: to, life: 0.07, max: 0.07, color: [1.0, 0.55, 0.25], width: 0.5 });
                }
            }
        }

        let mut killed: Vec<Vec3> = Vec::new();
        let mut shots: Vec<Vec3> = Vec::new();
        let mut sparks: Vec<Vec3> = Vec::new();
        let ship_pos = self.ship.pos;
        for i in 0..self.enemies.len() {
            let mut pos = self.enemies[i].pos;
            let mut cooldown = self.enemies[i].cooldown;
            let drift = self.enemies[i].drift;
            let to_player = ship_pos - pos;
            let dist = to_player.length().max(0.001);
            let dir = to_player / dist;
            if dist > 260.0 {
                pos += dir * 70.0 * dt;
            } else if dist < 150.0 {
                pos -= dir * 45.0 * dt;
            }
            pos.y += (self.spin * 0.6 + drift + i as f32).sin() * 12.0 * dt;

            cooldown -= dt;
            if dist < 520.0 && cooldown <= 0.0 {
                cooldown = 1.9;
                let dmg = 9.0;
                if self.shield > 0.0 {
                    self.shield = (self.shield - dmg).max(0.0);
                } else {
                    self.hull = (self.hull - dmg * 0.7).max(0.0);
                }
                self.hit_flash = 1.0;
                shots.push(pos);
                if self.rnd() < 0.5 {
                    sparks.push(ship_pos + dir * -10.0);
                }
            }
            let hp = self.enemies[i].hp;
            self.enemies[i].pos = pos;
            self.enemies[i].cooldown = cooldown;
            if hp <= 0.0 {
                killed.push(pos);
            }
        }
        for from in shots {
            if self.beams.len() < 24 {
                self.beams.push(Beam { a: from, b: ship_pos, life: 0.09, max: 0.09, color: [1.0, 0.28, 0.24], width: 0.7 });
            }
        }
        for at in sparks {
            self.explode(at, 3, [1.0, 0.42, 0.3], 24.0);
        }
        if !killed.is_empty() {
            self.enemies.retain(|e| e.hp > 0.0);
            for at in killed {
                self.kills += 1;
                self.credits += 350;
                self.contract_done += 1;
                self.explode(at, 40, [1.0, 0.55, 0.22], 90.0);
                self.explode(at, 16, [0.55, 0.68, 1.0], 60.0);
            }
            self.log = if self.contract_done >= self.contract_goal {
                "Pirate destroyed — contract ready at the station".to_string()
            } else {
                format!("Pirate destroyed: +350 cr  contract {}/{}", self.contract_done, self.contract_goal)
            };
        }

        self.shield = (self.shield + 3.0 * dt).min(60.0);

        // piraci wracają, żeby sektor nie opustoszał
        self.respawn -= dt;
        if self.respawn <= 0.0 && self.enemies.len() < 6 + self.sector * 3 {
            self.respawn = 22.0;
            let a = self.rnd() * std::f32::consts::TAU;
            let r = 900.0 + self.rnd() * 700.0;
            let pos = self.station_pos + Vec3::new(a.cos() * r * 0.6, 40.0 + self.rnd() * 160.0, a.sin() * r * 0.6);
            self.enemies.push(Enemy { pos, hp: 100.0, cooldown: 0.5, drift: a });
            self.log = "New pirate entered the sector".into();
        }

        if self.hull <= 0.0 {
            self.hull = 100.0;
            self.shield = 60.0;
            self.cargo = 0.0;
            self.alloy = 0.0;
            self.explode(self.ship.pos, 60, [1.0, 0.5, 0.2], 110.0);
            self.ship.pos = self.station_pos + Vec3::new(0.0, 0.0, 320.0);
            self.ship.speed = 30.0;
            self.hit_flash = 0.0;
            self.log = "Statek zniszczony — odrodzenie przy stacji, ruda i stop przepadly".into();
        }
    }

    fn dock(&mut self) {
        if self.ship.pos.distance(self.station_pos) < 900.0 {
            self.docked = true;
            self.ship.speed = 0.0;
            self.log = "Docking complete".into();
        } else {
            self.log = "Too far from the station".into();
        }
    }

    /// Handlujemy po `id`, nie po pozycji na liście: kolejność w market.txt może się
    /// zmienić, a klawisze muszą dalej oznaczać ten sam towar.
    fn trade(&mut self, id: &str, sell: bool) {
        let Some(item) = self.market.items.iter().find(|i| i.id == id).cloned() else {
            self.log = format!("No {id} on the market");
            return;
        };
        let price = station_price(if sell { item.sell } else { item.buy }, self.sector, id);
        if sell {
            // Ruda i stop to osobne ładownie; paliwa się nie sprzedaje.
            let held = match id {
                "ore_raw" => self.cargo,
                "ore_refined" => self.alloy,
                _ => 0.0,
            };
            let amount = held.floor() as u64;
            if amount == 0 {
                self.log = format!("No {} in cargo", item.name);
                return;
            }
            let gain = amount * price;
            match id {
                "ore_raw" => self.cargo -= amount as f32,
                "ore_refined" => self.alloy -= amount as f32,
                _ => {}
            }
            self.credits += gain;
            self.log = format!("Sold {amount} x {} for {gain} cr", item.name);
        } else if id == "fuel" {
            let cost = price;
            if self.credits < cost {
                self.log = "Not enough credits".into();
                return;
            }
            self.credits -= cost;
            self.fuel += 1;
            self.log = format!("Bought {} for {cost} cr", item.name);
        } else if id == "ore_raw" {
            let cost = price;
            if self.credits < cost {
                self.log = "Not enough credits".into();
                return;
            }
            if self.cargo + self.alloy * ALLOY_VOL + 5.0 > CARGO_CAP {
                self.log = "Cargo full".into();
                return;
            }
            self.credits -= cost;
            self.cargo += 5.0;
            self.log = format!("Bought {} for {cost} cr", item.name);
        } else {
            self.log = "Alloy is refined: press 5 to refine 10 ore".into();
        }
    }

    /// Rafineria stacyjna: 10 rudy -> 8 stopu, bez opłaty.
    fn refine(&mut self) {
        match refine_batch(self.cargo, self.alloy) {
            Ok((ore, alloy)) => {
                self.cargo = ore;
                self.alloy = alloy;
                self.explode(self.station_pos + Vec3::new(0.0, 90.0, 0.0), 10, [0.45, 0.82, 1.0], 30.0);
                self.log = format!("Rafinacja: {REFINE_IN:.0} rudy -> {REFINE_OUT:.0} stopu");
            }
            Err(RefineError::NoOre) => {
                self.log = format!("Rafinacja potrzebuje {REFINE_IN:.0} rudy");
            }
            Err(RefineError::NoSpace) => {
                self.log = "No cargo space for alloy".into();
            }
        }
    }

    fn claim_contract(&mut self) {
        if self.contract_done < self.contract_goal {
            self.log = format!("Contract: destroy {}/{} pirates", self.contract_done, self.contract_goal);
            return;
        }
        self.credits += self.contract_reward;
        self.log = format!("Contract paid: +{} cr", self.contract_reward);
        self.contract_done = 0;
        self.contract_goal += 2;
        self.contract_reward = 900 + (self.contract_goal as u64 - 3) * 450;
    }

    fn save(&mut self) {
        let text = format!(
            "VESPERSAVE 1\nsector {}\ncredits {}\ncargo {:.3}\nalloy {:.3}\nfuel {}\nkills {}\nhull {:.2}\nshield {:.2}\ncontract {} {} {}\npos {:.2} {:.2} {:.2}\nyaw {:.3}\npitch {:.3}\nroll {:.3}\n",
            self.sector,
            self.credits,
            self.cargo,
            self.alloy,
            self.fuel,
            self.kills,
            self.hull,
            self.shield,
            self.contract_goal,
            self.contract_done,
            self.contract_reward,
            self.ship.pos.x,
            self.ship.pos.y,
            self.ship.pos.z,
            self.ship.yaw,
            self.ship.pitch,
            self.ship.roll,
        );
        match std::fs::write("savegame.txt", text) {
            Ok(_) => self.log = "Zapisano savegame.txt (F9 wczytuje)".into(),
            Err(e) => self.log = format!("Save failed: {e}"),
        }
    }

    fn load(&mut self) {
        let text = match std::fs::read_to_string("savegame.txt") {
            Ok(t) => t,
            Err(_) => {
                self.log = "No savegame.txt".into();
                return;
            }
        };
        let mut sector = self.sector;
        let mut alloy = None;
        let mut pos = None;
        let mut yaw = None;
        let mut pitch = None;
        let mut roll = None;
        for line in text.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            let Some(key) = parts.first().copied() else { continue };
            let f = |i: usize| parts.get(i).and_then(|v| v.parse::<f32>().ok());
            let u = |i: usize| parts.get(i).and_then(|v| v.parse::<u64>().ok());
            match key {
                "sector" => sector = u(1).map(|v| v as usize).unwrap_or(sector),
                "credits" => self.credits = u(1).unwrap_or(self.credits),
                "cargo" => self.cargo = f(1).unwrap_or(self.cargo),
                "alloy" => alloy = f(1),
                "fuel" => self.fuel = u(1).map(|v| v as u32).unwrap_or(self.fuel),
                "kills" => self.kills = u(1).map(|v| v as u32).unwrap_or(self.kills),
                "hull" => self.hull = f(1).unwrap_or(self.hull),
                "shield" => self.shield = f(1).unwrap_or(self.shield),
                "contract" => {
                    self.contract_goal = u(1).map(|v| v as u32).unwrap_or(self.contract_goal);
                    self.contract_done = u(2).map(|v| v as u32).unwrap_or(self.contract_done);
                    self.contract_reward = u(3).unwrap_or(self.contract_reward);
                }
                "pos" => {
                    pos = Some(Vec3::new(f(1).unwrap_or(0.0), f(2).unwrap_or(0.0), f(3).unwrap_or(0.0)));
                }
                "yaw" => yaw = f(1),
                "pitch" => pitch = f(1),
                "roll" => roll = f(1),
                _ => {}
            }
        }
        self.docked = false;
        self.enter_sector(sector % self.sectors.len());
        self.alloy = alloy.unwrap_or(0.0);
        self.ship.pos = pos.unwrap_or(self.ship.pos);
        self.ship.yaw = yaw.unwrap_or(self.ship.yaw);
        self.ship.pitch = pitch.unwrap_or(self.ship.pitch);
        self.ship.roll = roll.unwrap_or(self.ship.roll);
        self.log = "Loaded savegame.txt".into();
    }

    fn hud(&self) -> Vec<UiRect> {
        let mut r = Vec::new();
        let w = self.gfx.width() as f32;
        let h = self.gfx.height() as f32;
        // jedna skala dla całego HUD: przy wąskim oknie wszystko się zmniejsza
        let u = (h / 720.0).min(w / 1280.0).clamp(0.72, 1.6);
        let m = 20.0 * u;

        if self.docked {
            r.push(UiRect::new(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.66]));
            let pw = (w - 2.0 * m).min(900.0 * u);
            let ph = (h - 2.0 * m).min(620.0 * u);
            let px0 = (w - pw) * 0.5;
            let py = (h - ph) * 0.5;
            r.push(UiRect::new(px0, py, pw, ph, [0.010, 0.014, 0.024, 0.95]));
            r.push(UiRect::new(px0, py, pw, 2.0 * u, ACCENT));
            r.push(UiRect::new(px0, py, 2.0 * u, ph, [0.16, 0.40, 0.48, 0.9]));
            let x = px0 + 30.0 * u;
            let right = px0 + pw - 30.0 * u;
            let mut y = py + 24.0 * u;

            ui::text(&mut r, x, y, &format!("STATION {}", self.market.station), 4.2 * u, ACCENT);
            y += 32.0 * u;
            ui::text(
                &mut r,
                x,
                y,
                &format!("SECTOR {} - {}   LOCAL MARKET", self.sectors[self.sector].name, self.sector + 1),
                2.1 * u,
                DIM,
            );
            y += 26.0 * u;
            r.push(UiRect::new(x, y, pw - 60.0 * u, 1.0, [1.0, 1.0, 1.0, 0.14]));
            y += 20.0 * u;

            let item = |id: &str| self.market.items.iter().find(|i| i.id == id);
            let ore_sell = item("ore_raw").map(|i| station_price(i.sell, self.sector, "ore_raw")).unwrap_or(0);
            let ore_buy = item("ore_raw").map(|i| station_price(i.buy, self.sector, "ore_raw")).unwrap_or(0);
            let fuel_buy = item("fuel").map(|i| station_price(i.buy, self.sector, "fuel")).unwrap_or(0);
            let alloy_sell = item("ore_refined").map(|i| station_price(i.sell, self.sector, "ore_refined")).unwrap_or(0);

            for (name, hint, price) in [
                ("ORE", "1 SELL   SHIFT+1 BUY 5", format!("{ore_sell} KR")),
                ("FUEL", "2 BUY 1", format!("{fuel_buy} KR")),
                ("ALLOY", "3 SELL", format!("{alloy_sell} KR")),
            ] {
                ui::text(&mut r, x, y, name, 3.2 * u, CREAM);
                ui::text(&mut r, x + 140.0 * u, y, hint, 2.2 * u, DIM);
                right_text(&mut r, right, y, &price, 2.6 * u, ACCENT);
                y += 30.0 * u;
            }
            ui::text(
                &mut r,
                x,
                y,
                &format!("5 RAFINERIA: {REFINE_IN:.0} RUDY -> {REFINE_OUT:.0} STOPU   SHIFT+1 KUP RUDE ZA {ore_buy} KR"),
                2.0 * u,
                WARN,
            );
            y += 26.0 * u;
            r.push(UiRect::new(x, y, pw - 60.0 * u, 1.0, [1.0, 1.0, 1.0, 0.14]));
            y += 22.0 * u;

            ui::text(
                &mut r,
                x,
                y,
                &format!("CREDITS {}   ORE {:.1}   ALLOY {:.1}   FUEL {}", self.credits, self.cargo, self.alloy, self.fuel),
                2.6 * u,
                CREAM,
            );
            y += 26.0 * u;
            let load = (self.cargo + self.alloy * ALLOY_VOL) / CARGO_CAP;
            ui::text(&mut r, x, y, &format!("CARGO {:.0}/{:.0} M3", load * CARGO_CAP, CARGO_CAP), 2.1 * u, DIM);
            hud_bar(&mut r, x, y + 14.0 * u, (pw - 60.0 * u).min(260.0 * u), 5.0 * u, load, ACCENT);
            y += 30.0 * u;

            let contract = if self.contract_done >= self.contract_goal {
                format!("CONTRACT READY: {} PIRATES - 4 CLAIM {} CR", self.contract_goal, self.contract_reward)
            } else {
                format!("CONTRACT {}/{}   REWARD {} CR", self.contract_done, self.contract_goal, self.contract_reward)
            };
            ui::text(
                &mut r,
                x,
                y,
                &contract,
                2.2 * u,
                if self.contract_done >= self.contract_goal { ACCENT } else { WARN },
            );
            y += 24.0 * u;
            ui::text(&mut r, x, y, &format!("KILLS {}   F5 SAVE   F9 LOAD", self.kills), 2.0 * u, DIM);
            y += 28.0 * u;
            ui::text(&mut r, x, y, "1-3 TRADE   SHIFT+1 BUY  5 REFINE  4 CONTRACT  G UNDOCK", 1.9 * u, DIM);
            y += 24.0 * u;
            if !self.log.is_empty() {
                ui::text(&mut r, x, y, &self.log, 2.2 * u, WARN);
            }
            return r;
        }

        // ---------------- HUD lotu ----------------
        let panel_w = 330.0 * u;
        let panel_h = 86.0 * u;
        r.push(UiRect::new(m, m, panel_w, panel_h, [0.015, 0.025, 0.04, 0.55]));
        r.push(UiRect::new(m, m, 2.0 * u, panel_h, [0.25, 0.65, 0.75, 0.75]));
        ui::text(&mut r, m + 12.0 * u, m + 10.0 * u, self.sectors[self.sector].name, 3.2 * u, ACCENT);
        right_text(
            &mut r,
            m + panel_w - 12.0 * u,
            m + 13.0 * u,
            &format!("SECTOR {}/{}", self.sector + 1, self.sectors.len()),
            1.8 * u,
            DIM,
        );

        let label_px = 1.8 * u;
        let bar_x = m + 12.0 * u + 7.0 * 4.0 * label_px;
        let bar_right = m + panel_w - 12.0 * u;
        let val_w = 4.0 * 4.0 * label_px;
        let bar_w = (bar_right - val_w - 8.0 * u - bar_x).max(40.0);
        let sy = m + 38.0 * u;
        ui::text(&mut r, m + 12.0 * u, sy, "SHIELD", label_px, DIM);
        right_text(&mut r, bar_right, sy, &format!("{:.0}", self.shield), label_px, CREAM);
        hud_bar(&mut r, bar_x, sy + 2.0 * u, bar_w, 5.0 * u, self.shield / 60.0, [0.28, 0.62, 0.95, 0.9]);
        let hy = sy + 20.0 * u;
        ui::text(&mut r, m + 12.0 * u, hy, "HULL", label_px, DIM);
        right_text(&mut r, bar_right, hy, &format!("{:.0}", self.hull), label_px, if self.hull < 40.0 { DANGER } else { CREAM });
        let hull_col = if self.hull < 40.0 { DANGER } else { [0.92, 0.52, 0.22, 0.9] };
        hud_bar(&mut r, bar_x, hy + 2.0 * u, bar_w, 5.0 * u, self.hull / 100.0, hull_col);

        // ekonomia po prawej
        let right = w - m;
        right_text(&mut r, right, m, &format!("{} KR", self.credits), 3.2 * u, CREAM);
        let load = (self.cargo + self.alloy * ALLOY_VOL) / CARGO_CAP;
        right_text(&mut r, right, m + 24.0 * u, &format!("CARGO {:.0}/{:.0} M3", load * CARGO_CAP, CARGO_CAP), 1.8 * u, DIM);
        let cw = 150.0 * u;
        hud_bar(&mut r, right - cw, m + 36.0 * u, cw, 5.0 * u, load, [0.35, 0.85, 0.95, 0.85]);
        right_text(
            &mut r,
            right,
            m + 50.0 * u,
            &format!("ORE {:.1}   ALLOY {:.1}   FUEL {}", self.cargo, self.alloy, self.fuel),
            2.0 * u,
            CREAM,
        );
        right_text(
            &mut r,
            right,
            m + 70.0 * u,
            &format!("CONTRACT {}/{}   KILLS {}", self.contract_done, self.contract_goal, self.kills),
            1.9 * u,
            if self.contract_done >= self.contract_goal { ACCENT } else { DIM },
        );

        // prędkość i ciąg
        let speed_s = format!("{:.0}", self.ship.speed);
        let sy = h - m - 64.0 * u;
        ui::text(&mut r, m, sy, &speed_s, 5.0 * u, ACCENT);
        ui::text(&mut r, m + ui::text_width(&speed_s, 5.0 * u) + 6.0 * u, sy + 16.0 * u, "M/S", 1.8 * u, DIM);
        hud_bar(&mut r, m, sy + 34.0 * u, 160.0 * u, 4.0 * u, self.ship.speed / 900.0, ACCENT);

        // stacja po prawej na dole
        let dist = self.ship.pos.distance(self.station_pos);
        let near = dist < 1200.0;
        right_text(
            &mut r,
            right,
            h - m - 34.0 * u,
            &format!("STATION {:.0} M", dist),
            2.0 * u,
            if near { ACCENT } else { DIM },
        );
        if near {
            right_text(&mut r, right, h - m - 16.0 * u, "G DOCK", 2.4 * u, ACCENT);
        }

        // celownik
        let cx = w * 0.5;
        let cy = h * 0.5;
        let locked = self.target.is_some();
        let col = if locked { [0.95, 0.62, 0.30, 0.9] } else { [0.62, 0.74, 0.85, 0.38] };
        let gap = 4.0 * u;
        let arm = 7.0 * u;
        for (dx, dy) in [(-1.0f32, 0.0f32), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
            let x = if dx < 0.0 { cx - gap - arm } else if dx > 0.0 { cx + gap } else { cx - 1.0 * u };
            let y = if dy < 0.0 { cy - gap - arm } else if dy > 0.0 { cy + gap } else { cy - 1.0 * u };
            r.push(UiRect::new(x, y, if dy == 0.0 { arm } else { 2.0 * u }, if dx == 0.0 { arm } else { 2.0 * u }, col));
        }
        if locked {
            for (sx, sy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let bx = cx + sx * 26.0 * u;
                let by = cy + sy * 26.0 * u;
                r.push(UiRect::new(bx - sx * 8.0 * u, by, 8.0 * u, 1.5 * u, [0.95, 0.62, 0.30, 0.55]));
                r.push(UiRect::new(bx, by - sy * 8.0 * u, 1.5 * u, 8.0 * u, [0.95, 0.62, 0.30, 0.55]));
            }
        }
        let target_s = match self.target {
            Some(i) => {
                if self.laser > 0.0 {
                    format!("ASTEROID R {:.0}   MINING", self.rocks[i].radius)
                } else {
                    format!("ASTEROID R {:.0}   F MINE", self.rocks[i].radius)
                }
            }
            None => "NO TARGET".to_string(),
        };
        let tw = ui::text_width(&target_s, 2.0 * u);
        // ciemna listwa pod etykietą celu — nad jasną stacją tekst gubił się w tle
        r.push(UiRect::new(
            cx - tw * 0.5 - 5.0 * u,
            cy + 34.0 * u,
            tw + 10.0 * u,
            15.0 * u,
            [0.0, 0.0, 0.0, 0.42],
        ));
        ui::text(&mut r, cx - tw * 0.5, cy + 38.0 * u, &target_s, 2.0 * u, if locked { WARN } else { DIM });

        // ostrzeżenie o piratach i log
        if !self.enemies.is_empty() {
            let nearest = self
                .enemies
                .iter()
                .map(|e| e.pos.distance(self.ship.pos))
                .fold(f32::MAX, f32::min);
            let s = format!("HOSTILES {}   NEAREST {:.0} M   SPACE FIRE", self.enemies.len(), nearest);
            let tw = ui::text_width(&s, 2.0 * u);
            ui::text(&mut r, (w - tw) * 0.5, m + 96.0 * u, &s, 2.0 * u, WARN);
        }
        if !self.log.is_empty() {
            let tw = ui::text_width(&self.log, 1.9 * u);
            ui::text(&mut r, (w - tw) * 0.5, h - m - 42.0 * u, &self.log, 1.9 * u, DIM);
        }
        let controls = "F MINE   SPACE FIRE   G DOCK   H JUMP   F5/F9";
        let cw = ui::text_width(controls, 1.7 * u);
        ui::text(&mut r, (w - cw) * 0.5, h - m - 12.0 * u, controls, 1.7 * u, DIM);

        if self.hit_flash > 0.0 {
            let a = 0.35 * self.hit_flash;
            r.push(UiRect::new(0.0, 0.0, w, 4.0, [0.9, 0.2, 0.2, a]));
            r.push(UiRect::new(0.0, h - 4.0, w, 4.0, [0.9, 0.2, 0.2, a]));
        }
        if self.flash > 0.0 {
            r.push(UiRect::new(0.0, 0.0, w, h, [0.55, 0.85, 1.0, 0.45 * self.flash]));
        }
        r
    }

    fn render(&mut self) {
        let shake = self.hit_flash * 1.5;
        let eye = self.cam_pos
            + Vec3::new((self.spin * 47.0).sin(), (self.spin * 61.0).cos() * 0.6, 0.0) * shake;
        let target = self.cam_target;
        let up = self.cam_up;
        let fov = self.cam_fov;
        let proj = Mat4::perspective_rh(fov.to_radians(), self.gfx.aspect(), 0.5, 40000.0);
        let view = Mat4::look_at_rh(eye, target, up);
        let vp = proj * view;

        // baza kamery do billboardów gwiazd i silników
        let dir = (target - eye).normalize_or_zero();
        let right = dir.cross(up).normalize_or_zero();
        let up2 = right.cross(dir);

        let ship_inst = vec![Instance::new(self.ship.model(), [0.44, 0.47, 0.52, 1.0])];
        let t = self.station_tint;
        let station_model = Mat4::from_translation(self.station_pos)
            * Mat4::from_rotation_y(self.spin * 0.05)
            * Mat4::from_scale(Vec3::splat(STATION_SCALE));
        let station_inst = vec![Instance::new(station_model, [t[0], t[1], t[2], 0.6])];

        let mut rock_map: std::collections::HashMap<usize, Vec<Instance>> = Default::default();
        for (i, r) in self.rocks.iter().enumerate() {
            if r.left <= 0.0 {
                continue;
            }
            let rot = Mat4::from_rotation_y(self.spin * 0.1 + r.radius)
                * Mat4::from_rotation_x(self.spin * 0.07 + r.pos.x);
            let hot = if self.target == Some(i) && self.laser > 0.0 {
                [0.95, 0.55, 0.28, 0.0]
            } else if self.target == Some(i) {
                [0.62, 0.55, 0.44, 0.0]
            } else {
                [0.40, 0.38, 0.35, 0.0]
            };
            rock_map.entry(r.mesh).or_default().push(Instance::new(Mat4::from_translation(r.pos) * rot, hot));
        }

        let enemy_inst: Vec<Instance> = self
            .enemies
            .iter()
            .map(|e| {
                let dir = (self.ship.pos - e.pos).normalize_or_zero();
                let yaw = dir.z.atan2(dir.x);
                Instance::new(
                    Mat4::from_translation(e.pos)
                        * Mat4::from_rotation_y(-yaw)
                        * Mat4::from_scale(Vec3::splat(0.85)),
                    [0.60, 0.22, 0.20, 1.0],
                )
            })
            .collect();

        let mut batches = vec![
            (self.ship_mesh, ship_inst),
            (self.station_mesh, station_inst),
            (self.ship_mesh, enemy_inst),
        ];
        for (mi, list) in rock_map {
            batches.push((mi, list));
        }

        // --- sprite'y: gwiazdy, silniki, światła stacji, cząstki ---
        let mut sprites: Vec<Instance> = Vec::with_capacity(2600 + self.particles.len());
        let star_r = 12_000.0;
        for (pos, size, bright, color) in &self.stars {
            let p = eye + pos.normalize_or_zero() * star_r;
            let a = *bright * 0.9;
            sprites.push(Instance::new(
                billboard(p, right, up2, dir, *size, *size),
                [color[0], color[1], color[2], a],
            ));
            if *bright > 0.62 {
                sprites.push(Instance::new(
                    billboard(p, right, up2, dir, size * 3.2, size * 3.2),
                    [color[0], color[1], color[2], a * 0.10],
                ));
            }
            if *bright > 0.94 {
                let flare = [color[0], color[1], color[2], a * 0.30];
                sprites.push(Instance::new(billboard(p, right, up2, dir, size * 6.0, size * 0.30), flare));
                sprites.push(Instance::new(billboard(p, right, up2, dir, size * 0.30, size * 6.0), flare));
            }
        }

        if !self.docked {
            let throttle = (self.ship.speed / 900.0).clamp(0.0, 1.0);
            let model = self.ship.model();
            let flicker = 0.85 + 0.15 * (self.spin * 41.0).sin();
            for side in [-1.0f32, 1.0] {
                let local = Vec3::new(-24.0, 0.0, side * 4.4);
                let p = model.transform_point3(local);
                let size = 2.4 + throttle * 3.2;
                sprites.push(Instance::new(
                    billboard(p, right, up2, dir, size, size),
                    [0.30, 0.62, 1.0, (0.30 + throttle * 0.55) * flicker],
                ));
                sprites.push(Instance::new(
                    billboard(p, right, up2, dir, size * 3.0, size * 3.0),
                    [0.12, 0.30, 0.85, (0.07 + throttle * 0.10) * flicker],
                ));
            }
        }

        // światła stacji: pierścień, doki i czerwone światła ostrzegawcze
        let ring_lights = 36;
        for i in 0..ring_lights {
            let a = i as f32 / ring_lights as f32 * std::f32::consts::TAU;                let local = Vec3::new(a.cos() * 124.0, 7.0, a.sin() * 124.0);
            let p = station_model.transform_point3(local);
            let c = if i % 6 == 0 { [0.45, 0.85, 1.0] } else { [0.30, 0.55, 0.85] };
            sprites.push(Instance::new(
                billboard(p, right, up2, dir, 4.0, 4.0),
                [c[0], c[1], c[2], 0.55],
            ));
        }
        for k in 0..4 {
            let a = k as f32 * std::f32::consts::FRAC_PI_2 + std::f32::consts::FRAC_PI_4;
            let local = Vec3::new(a.cos() * 138.0, 0.0, a.sin() * 138.0);
            let p = station_model.transform_point3(local);
            let blink = 0.35 + 0.65 * ((self.spin * 3.0 + k as f32).sin() * 0.5 + 0.5);
            sprites.push(Instance::new(
                billboard(p, right, up2, dir, 6.0, 6.0),
                [1.0, 0.20, 0.14, blink * 0.8],
            ));
        }
        sprites.push(Instance::new(
            billboard(station_model.transform_point3(Vec3::new(0.0, 34.0, 0.0)), right, up2, dir, 26.0, 26.0),
            [1.0, 0.72, 0.38, 0.16],
        ));

        // Słońce: odległa gwiazda w górnej części kadru — daje bloom i kierunek światła.
        let sun_dir = (dir * 0.92 + up2 * 0.36 + right * 0.22).normalize_or_zero();
        let sun_p = eye + sun_dir * 18000.0;
        sprites.push(Instance::new(
            billboard(sun_p, right, up2, dir, 1900.0, 1900.0),
            [1.0, 0.72, 0.44, 0.12],
        ));
        sprites.push(Instance::new(
            billboard(sun_p, right, up2, dir, 620.0, 620.0),
            [3.4, 3.0, 2.5, 1.0],
        ));

        for p in &self.particles {
            let k = (p.life / p.max).clamp(0.0, 1.0);
            let s = p.size * (0.5 + 0.8 * k);
            sprites.push(Instance::new(
                billboard(p.pos, right, up2, dir, s, s),
                [p.color[0], p.color[1], p.color[2], k * 0.75],
            ));
        }

        // --- additive: wiązki ---
        let mut glow_inst: Vec<Instance> = Vec::with_capacity(self.beams.len());
        for b in &self.beams {
            let k = (b.life / b.max).clamp(0.0, 1.0);
            let dir = (b.b - b.a).normalize_or_zero();
            let len = b.a.distance(b.b).max(0.5);
            let q = Quat::from_rotation_arc(Vec3::X, dir);
            let model = Mat4::from_translation((b.a + b.b) * 0.5)
                * Mat4::from_quat(q)
                * Mat4::from_scale(Vec3::new(len, b.width, b.width));
            glow_inst.push(Instance::new(model, [b.color[0], b.color[1], b.color[2], k]));
        }
        let glow = vec![(self.cube_mesh, glow_inst)];
        let sprite_batches = vec![(self.quad_mesh, sprites)];

        let ui = self.hud();
        self.gfx.render(vp, eye, target, up, fov, &batches, &glow, &sprite_batches, &ui);

        self.window.set_title(&format!(
            "VESPER - {}  |  {}  |  {} KR",
            if self.docked { "DOCKED" } else { "FLIGHT" },
            self.sectors[self.sector].name,
            self.credits,
        ));
    }

    fn on_key(&mut self, code: KeyCode, pressed: bool) {
        if pressed {
            self.keys.insert(code);
            match code {
                KeyCode::F11 => {
                    self.fullscreen = !self.fullscreen;
                    self.window.set_fullscreen(if self.fullscreen {
                        Some(Fullscreen::Borderless(None))
                    } else {
                        None
                    });
                }
                KeyCode::KeyG => {
                    if self.docked {
                        self.docked = false;
                        self.ship.speed = 40.0;
                        self.log = "Undocked — free flight".into();
                    } else {
                        self.dock();
                    }
                }
                KeyCode::KeyH => self.warp(),
                KeyCode::F5 => self.save(),
                KeyCode::F9 => self.load(),
                KeyCode::Digit1 if self.docked => self.trade("ore_raw", !self.shift),
                KeyCode::Digit2 if self.docked => self.trade("fuel", false),
                KeyCode::Digit3 if self.docked => self.trade("ore_refined", true),
                KeyCode::Digit4 if self.docked => self.claim_contract(),
                KeyCode::Digit5 if self.docked => self.refine(),
                _ => {}
            }
        } else {
            self.keys.remove(&code);
        }
    }
}

/// Buduje układ sektora: pozycje asteroid (z gotowymi siatkami) i startowe pozycje piratów.
fn build_sector(
    gfx: &mut Renderer,
    name: &'static str,
    station: Vec3,
    tint: [f32; 3],
    offset: u32,
    rock_count: usize,
    enemy_count: usize,
) -> SectorLayout {
    let mut rocks = Vec::new();
    let mut seed = 0x9E37_79B9u32.wrapping_mul(offset + 1);
    for _ in 0..rock_count {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let r = 40.0 + (seed % 500) as f32;
        let a = (seed % 6283) as f32 / 1000.0;
        let b = ((seed >> 8) % 6283) as f32 / 1000.0;
        let pos = station + Vec3::new(a.cos() * r * 6.0, b.sin() * 220.0, b.cos() * r * 6.0 + 700.0);
        let radius = 8.0 + (seed >> 16) as f32 % 55.0;
        let mi = gfx.add_mesh(&MeshData::asteroid(seed, radius, 0.45));
        rocks.push(RockSpawn { pos, radius, mesh: mi });
    }

    let mut enemies = Vec::new();
    let mut eseed = 99u32.wrapping_add(offset * 37);
    for i in 0..enemy_count {
        eseed = eseed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let a = (eseed % 6283) as f32 / 1000.0;
        let r = 900.0 + (eseed % 700) as f32;
        enemies.push(station + Vec3::new(a.cos() * r * 0.6, 40.0 * (i as f32 % 5.0), a.sin() * r * 0.6 - 400.0));
    }

    SectorLayout { name, station, tint, rocks, enemies }
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
        let window = Arc::new(event_loop.create_window(attrs).expect("no window"));
        self.state = Some(pollster::block_on(State::new(window)));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(state) = self.state.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested => {
                // kontekst GPU musi zostać zwolniony, zanim zniknie połączenie Wayland
                self.state = None;
                event_loop.exit();
            }
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
                    self.state = None;
                    event_loop.exit();
                    return;
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
    let event_loop = EventLoop::new().expect("no event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App { state: None };
    event_loop.run_app(&mut app).expect("event loop failed");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rafineria_daje_stop_z_partii_rudy() {
        assert_eq!(refine_batch(10.0, 0.0), Ok((0.0, 8.0)));
        assert_eq!(refine_batch(34.0, 2.0), Ok((24.0, 10.0)));
    }

    #[test]
    fn rafineria_odrzuca_niepelna_partie() {
        assert_eq!(refine_batch(9.999, 5.0), Err(RefineError::NoOre));
    }

    #[test]
    fn ceny_zaleza_od_sektora() {
        assert_eq!(station_price(55, 0, "fuel"), 45);
        assert_eq!(station_price(55, 1, "fuel"), 70);
        assert_eq!(station_price(26, 0, "ore_refined"), 22);
        assert_eq!(station_price(26, 1, "ore_refined"), 32);
        assert_eq!(station_price(1, 1, "nieznany"), 1);
    }

    #[test]
    fn rafineria_pilnuje_pojemnosci() {
        // 10 rudy znika, ale 8 stopu po 0,6 m³ nie zmieści się obok 330 stopu.
        assert_eq!(refine_batch(10.0, 330.0), Err(RefineError::NoSpace));
    }
}
