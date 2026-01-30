//! Galaga Space Shooter Game Plugin
//!
//! A native Rust implementation of classic space shooter game.
//! Features:
//! - Pixel-perfect rendering via Kitty graphics protocol
//! - Smooth 60fps gameplay
//! - Multiple enemy types with unique movement patterns
//! - Formation-based enemy AI (turtle, dive, side-to-side)
//! - Starfield parallax background
//! - Particle explosion effects
//! - Combo system and scoring

use std::any::Any;
use std::sync::Arc;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    buffer::Buffer, layout::Rect, style::Color, style::Style, text::Line, text::Span,
    widgets::Paragraph, widgets::Widget,
};

use super::{Plugin, PluginContext, PluginEventResult, PluginFrame, PluginRenderMode};

// ============================================================================
// CONSTANTS & CONFIGURATION
// ============================================================================

/// Internal game resolution
pub const GAME_WIDTH: u32 = 640;
pub const GAME_HEIGHT: u32 = 480;

/// Player ship configuration
const SHIP_WIDTH: f32 = 32.0;
const SHIP_HEIGHT: f32 = 24.0;
const SHIP_SPEED: f32 = 400.0;
const SHIP_Y: f32 = GAME_HEIGHT as f32 - 50.0;
const FIRE_COOLDOWN: f32 = 0.15; // Seconds between shots

/// Bullet configuration
const BULLET_SPEED: f32 = 600.0;
const BULLET_WIDTH: f32 = 4.0;
const BULLET_HEIGHT: f32 = 12.0;
const ENEMY_BULLET_SPEED: f32 = 300.0;

/// Enemy configuration
const ENEMY_ROWS: usize = 5;
const ENEMY_COLS: usize = 10;
const ENEMY_WIDTH: f32 = 24.0;
const ENEMY_HEIGHT: f32 = 20.0;
const ENEMY_PADDING: f32 = 8.0;
const ENEMY_LEFT_OFFSET: f32 = 50.0;
const ENEMY_TOP_OFFSET: f32 = 50.0;

/// Formation movement
const FORMATION_MOVE_SPEED: f32 = 60.0;
const FORMATION_DROP_AMOUNT: f32 = 20.0;

/// Game configuration
const INITIAL_LIVES: u8 = 3;
const MAX_PARTICLES: usize = 200;

// ============================================================================
// COLOR PALETTE
// ============================================================================

/// Enemy type colors
const ENEMY_COLORS: &[u32] = &[
    0xFF0000, // Boss - Red
    0xFF7F00, // Butterfly - Orange
    0x00FF00, // Bee - Green
    0xFFFF00, // Commet - Yellow
];

/// Point values per enemy type
const ENEMY_POINTS: &[u32] = &[50, 80, 40, 30];

// ============================================================================
// GAME STATE
// ============================================================================

/// Main game states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameState {
    Menu,
    Playing,
    Paused,
    GameOver,
    LevelComplete,
    StageClear,
}

/// Enemy types (based on Galaga)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyType {
    Boss,      // Red - 50 pts
    Butterfly, // Orange - 80 pts
    Bee,       // Green - 40 pts
    Commet,    // Yellow - 30 pts
}

/// Individual enemy
#[derive(Debug, Clone)]
struct Enemy {
    x: f32,
    y: f32,
    enemy_type: EnemyType,
    alive: bool,
    in_formation: bool,
    diving: bool,
    dive_target_x: f32,
    dive_target_y: f32,
    color: u32,
}

impl Enemy {
    fn new(col: usize, row: usize, enemy_type: EnemyType) -> Self {
        let x = ENEMY_LEFT_OFFSET + col as f32 * (ENEMY_WIDTH + ENEMY_PADDING);
        let y = ENEMY_TOP_OFFSET + row as f32 * (ENEMY_HEIGHT + ENEMY_PADDING);
        let color = ENEMY_COLORS[enemy_type as usize];

        Self {
            x,
            y,
            enemy_type,
            alive: true,
            in_formation: true,
            diving: false,
            dive_target_x: 0.0,
            dive_target_y: 0.0,
            color,
        }
    }

    fn get_points(&self) -> u32 {
        ENEMY_POINTS[self.enemy_type as usize]
    }
}

/// Player ship
#[derive(Debug, Clone)]
struct Player {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    velocity: f32,
    fire_timer: f32,
    invincible: bool,
    invincible_timer: f32,
}

impl Player {
    fn new() -> Self {
        Self {
            x: GAME_WIDTH as f32 / 2.0 - SHIP_WIDTH / 2.0,
            y: SHIP_Y,
            width: SHIP_WIDTH,
            height: SHIP_HEIGHT,
            velocity: 0.0,
            fire_timer: 0.0,
            invincible: false,
            invincible_timer: 0.0,
        }
    }

    fn update(&mut self, dt: f32, keys: &KeyState) {
        // Movement with smooth acceleration
        let mut target_velocity = 0.0;
        if keys.left {
            target_velocity -= SHIP_SPEED;
        }
        if keys.right {
            target_velocity += SHIP_SPEED;
        }

        // Smooth movement
        self.velocity = target_velocity;
        self.x += self.velocity * dt;

        // Clamp to screen bounds
        self.x = self.x.clamp(0.0, GAME_WIDTH as f32 - self.width);

        // Fire cooldown
        if self.fire_timer > 0.0 {
            self.fire_timer -= dt;
        }

        // Invincibility timer
        if self.invincible {
            self.invincible_timer -= dt;
            if self.invincible_timer <= 0.0 {
                self.invincible = false;
            }
        }
    }

    fn fire(&mut self) -> Option<Bullet> {
        if self.fire_timer <= 0.0 {
            self.fire_timer = FIRE_COOLDOWN;
            Some(Bullet::new(
                self.x + self.width / 2.0 - BULLET_WIDTH / 2.0,
                self.y,
                -BULLET_SPEED,
            ))
        } else {
            None
        }
    }

    fn hit(&mut self) -> bool {
        if !self.invincible {
            self.invincible = true;
            self.invincible = true;
            self.invincible_timer = 2.0;
            true
        } else {
            false
        }
    }

    fn get_rect(&self) -> (f32, f32, f32, f32) {
        (self.x, self.y, self.width, self.height)
    }
}

/// Bullet (player or enemy)
#[derive(Debug, Clone)]
struct Bullet {
    x: f32,
    y: f32,
    vy: f32,
    alive: bool,
}

impl Bullet {
    fn new(x: f32, y: f32, vy: f32) -> Self {
        Self {
            x,
            y,
            vy,
            alive: true,
        }
    }

    fn update(&mut self, dt: f32) {
        self.y += self.vy * dt;

        if self.y < -BULLET_HEIGHT || self.y > GAME_HEIGHT as f32 {
            self.alive = false;
        }
    }
}

/// Star for background
#[derive(Debug, Clone)]
struct Star {
    x: f32,
    y: f32,
    speed: f32,
    brightness: u8,
    size: f32,
}

impl Star {
    fn new() -> Self {
        let speed_multiplier = rand::random::<f32>();
        Self {
            x: rand::random::<f32>() * GAME_WIDTH as f32,
            y: rand::random::<f32>() * GAME_HEIGHT as f32,
            speed: 20.0 + speed_multiplier * 80.0,
            brightness: (100.0 + rand::random::<f32>() * 155.0) as u8,
            size: 1.0 + speed_multiplier * 1.5,
        }
    }

    fn update(&mut self, dt: f32) {
        self.y += self.speed * dt;

        if self.y > GAME_HEIGHT as f32 {
            self.y = 0.0;
            self.x = rand::random::<f32>() * GAME_WIDTH as f32;
        }
    }
}

/// Particle for explosions
#[derive(Debug, Clone)]
struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
    max_life: f32,
    color: u32,
    size: f32,
}

impl Particle {
    fn new(x: f32, y: f32, color: u32) -> Self {
        let angle = rand::random::<f32>() * std::f32::consts::PI * 2.0;
        let speed = rand::random::<f32>() * 300.0 + 100.0;
        Self {
            x,
            y,
            vx: angle.cos() * speed,
            vy: angle.sin() * speed,
            life: rand::random::<f32>() * 0.4 + 0.2,
            max_life: 0.6,
            color,
            size: rand::random::<f32>() * 4.0 + 2.0,
        }
    }

    fn update(&mut self, dt: f32) {
        self.x += self.vx * dt;
        self.y += self.vy * dt;
        self.vy += 200.0 * dt; // Light gravity
        self.life -= dt;
    }
}

/// Keyboard state tracking
#[derive(Debug, Clone, Default)]
struct KeyState {
    left: bool,
    right: bool,
    fire: bool,
}

/// Enemy formation state
#[derive(Debug, Clone)]
struct FormationState {
    direction: i8, // 1 = right, -1 = left
    drop_pending: bool,
    min_x: f32,
    max_x: f32,
}

impl Default for FormationState {
    fn default() -> Self {
        Self {
            direction: 1,
            drop_pending: false,
            min_x: ENEMY_LEFT_OFFSET,
            max_x: ENEMY_LEFT_OFFSET + (ENEMY_COLS as f32) * (ENEMY_WIDTH + ENEMY_PADDING)
                - ENEMY_PADDING,
        }
    }
}

// ============================================================================
// MAIN PLUGIN STRUCT
// ============================================================================

pub struct GalagaPlugin {
    // Game state
    state: GameState,
    stage: u8,
    wave: u8,

    // Game objects
    player: Player,
    enemies: Vec<Enemy>,
    bullets: Vec<Bullet>,
    enemy_bullets: Vec<Bullet>,
    particles: Vec<Particle>,
    stars: Vec<Star>,

    // Formation state
    formation: FormationState,

    // Score and lives
    score: u32,
    lives: u8,
    high_score: u32,
    combo: u32,
    combo_timer: f32,

    // Input state
    keys: KeyState,

    // Rendering - Arc for zero-copy sharing with graphics system
    frame_buffer: Arc<Vec<u8>>,
    scratch_buffer: Vec<u8>,
    frame_ready: bool,

    // Level data
    enemy_grid: Vec<Vec<EnemyType>>,
}

impl GalagaPlugin {
    pub fn new() -> Self {
        let size = (GAME_WIDTH * GAME_HEIGHT * 4) as usize;

        // Create starfield
        let mut stars = Vec::with_capacity(100);
        for _ in 0..100 {
            stars.push(Star::new());
        }

        let mut plugin = Self {
            state: GameState::Menu,
            stage: 1,
            wave: 1,
            player: Player::new(),
            enemies: Vec::new(),
            bullets: Vec::new(),
            enemy_bullets: Vec::new(),
            particles: Vec::new(),
            stars,
            formation: FormationState::default(),
            score: 0,
            lives: INITIAL_LIVES,
            high_score: 0,
            combo: 0,
            combo_timer: 0.0,
            keys: KeyState::default(),
            frame_buffer: Arc::new(Vec::new()),
            scratch_buffer: Vec::with_capacity(size),
            frame_ready: false,
            enemy_grid: Vec::new(),
        };

        plugin.generate_enemy_grid(1);
        plugin.load_stage();
        plugin
    }

    fn generate_enemy_grid(&mut self, level: u8) {
        self.enemy_grid = Vec::with_capacity(ENEMY_ROWS);

        for row in 0..ENEMY_ROWS {
            let mut row_grid = Vec::with_capacity(ENEMY_COLS);
            for _col in 0..ENEMY_COLS {
                let enemy_type = match (row, level % 3) {
                    (0, _) => EnemyType::Boss,
                    (1, _) => EnemyType::Butterfly,
                    (2, _) => EnemyType::Bee,
                    _ => EnemyType::Commet,
                };
                row_grid.push(enemy_type);
            }
            self.enemy_grid.push(row_grid);
        }
    }

    fn load_stage(&mut self) {
        self.enemies.clear();
        self.bullets.clear();
        self.enemy_bullets.clear();
        self.particles.clear();

        // Create enemy formation
        for row in 0..ENEMY_ROWS {
            for col in 0..ENEMY_COLS {
                let enemy_type = self.enemy_grid[row][col];
                self.enemies.push(Enemy::new(col, row, enemy_type));
            }
        }

        self.formation = FormationState::default();
        self.player = Player::new();
    }

    fn reset_game(&mut self) {
        self.score = 0;
        self.lives = INITIAL_LIVES;
        self.stage = 1;
        self.wave = 1;
        self.combo = 0;
        self.combo_timer = 0.0;
        self.generate_enemy_grid(1);
        self.load_stage();
        self.state = GameState::Playing;
    }

    fn update_formation(&mut self, dt: f32) {
        let move_amount = FORMATION_MOVE_SPEED * dt;

        // Check if formation should change direction or drop
        for enemy in &self.enemies {
            if !enemy.alive || !enemy.in_formation {
                continue;
            }

            if enemy.x <= self.formation.min_x || enemy.x + ENEMY_WIDTH >= self.formation.max_x {
                self.formation.drop_pending = true;
                return;
            }
        }

        // Move formation
        if self.formation.drop_pending {
            for enemy in &mut self.enemies {
                if enemy.alive && enemy.in_formation {
                    enemy.y += FORMATION_DROP_AMOUNT;
                    enemy.x += move_amount * self.formation.direction as f32;
                }
            }
            self.formation.direction *= -1;
            self.formation.drop_pending = false;
        } else {
            for enemy in &mut self.enemies {
                if enemy.alive && enemy.in_formation {
                    enemy.x += move_amount * self.formation.direction as f32;
                }
            }
        }
    }

    fn update_enemies(&mut self, dt: f32) {
        // Update diving enemies
        for enemy in &mut self.enemies {
            if !enemy.alive || !enemy.diving {
                continue;
            }

            // Move towards dive target
            let dx = enemy.dive_target_x - enemy.x;
            let dy = enemy.dive_target_y - enemy.y;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist < 5.0 {
                enemy.diving = false;
                enemy.in_formation = true;
                enemy.y = ENEMY_TOP_OFFSET; // Return to formation position
            } else {
                let speed = 200.0;
                enemy.x += (dx / dist) * speed * dt;
                enemy.y += (dy / dist) * speed * dt;
            }

            // Maybe fire
            if rand::random::<f32>() < 0.005 {
                self.enemy_bullets.push(Bullet::new(
                    enemy.x + ENEMY_WIDTH / 2.0 - BULLET_WIDTH / 2.0,
                    enemy.y + ENEMY_HEIGHT,
                    ENEMY_BULLET_SPEED,
                ));
            }
        }
    }

    fn start_enemy_dive(enemy: &mut Enemy) {
        if !enemy.alive || enemy.diving || !enemy.in_formation {
            return;
        }

        enemy.in_formation = false;
        enemy.diving = true;
        enemy.dive_target_x = rand::random::<f32>() * (GAME_WIDTH as f32 - ENEMY_WIDTH);
        enemy.dive_target_y = GAME_HEIGHT as f32 - 50.0;
    }

    fn update(&mut self, dt: f32) {
        match self.state {
            GameState::Playing => {
                // Update player
                self.player.update(dt, &self.keys);

                // Handle firing
                if self.keys.fire && self.player.fire_timer <= 0.0 {
                    if let Some(bullet) = self.player.fire() {
                        self.bullets.push(bullet);
                    }
                }

                // Update bullets
                for bullet in &mut self.bullets {
                    bullet.update(dt);
                }
                self.bullets.retain(|b| b.alive);

                // Update enemy bullets
                for bullet in &mut self.enemy_bullets {
                    bullet.update(dt);
                }
                self.enemy_bullets.retain(|b| b.alive);

                // Update stars
                for star in &mut self.stars {
                    star.update(dt);
                }

                // Update particles
                for particle in &mut self.particles {
                    particle.update(dt);
                }
                self.particles.retain(|p| p.life > 0.0);
                if self.particles.len() > MAX_PARTICLES {
                    self.particles.truncate(MAX_PARTICLES);
                }

                // Update formation
                self.update_formation(dt);

                // Update enemies
                self.update_enemies(dt);

                // Random dive starts
                if rand::random::<f32>() < 0.01 {
                    if let Some(diver) = self.enemies.iter_mut().find(|e| e.alive && e.in_formation)
                    {
                        Self::start_enemy_dive(diver);
                    }
                }

                // Collision detection
                self.check_collisions();

                // Check player death
                if self.player.hit() {
                    // Spawn explosion
                    for _ in 0..30 {
                        self.particles.push(Particle::new(
                            self.player.x + SHIP_WIDTH / 2.0,
                            self.player.y + SHIP_HEIGHT / 2.0,
                            0xFF6600,
                        ));
                    }
                    self.combo = 0;

                    // Check lives
                    if self.lives == 0 {
                        self.state = GameState::GameOver;
                        if self.score > self.high_score {
                            self.high_score = self.score;
                        }
                    }
                }

                // Update combo timer
                if self.combo_timer > 0.0 {
                    self.combo_timer -= dt;
                    if self.combo_timer <= 0.0 {
                        self.combo = 0;
                    }
                }

                // Check level complete
                if self.enemies.iter().all(|e| !e.alive) {
                    if self.stage >= 3 {
                        self.state = GameState::StageClear;
                    } else {
                        self.state = GameState::LevelComplete;
                    }
                }

                self.frame_ready = true;
            }
            GameState::Menu
            | GameState::Paused
            | GameState::GameOver
            | GameState::LevelComplete
            | GameState::StageClear => {}
        }
    }

    fn check_collisions(&mut self) {
        let (px, py, pw, ph) = self.player.get_rect();

        // Player bullets vs enemies
        for bullet in &mut self.bullets {
            if !bullet.alive {
                continue;
            }

            for enemy in &mut self.enemies {
                if !enemy.alive {
                    continue;
                }

                if bullet.x < enemy.x + ENEMY_WIDTH
                    && bullet.x + BULLET_WIDTH > enemy.x
                    && bullet.y < enemy.y + ENEMY_HEIGHT
                    && bullet.y + BULLET_HEIGHT > enemy.y
                {
                    bullet.alive = false;
                    enemy.alive = false;

                    // Score and combo
                    self.combo += 1;
                    self.combo_timer = 3.0;
                    let points = enemy.get_points() * (1 + self.combo / 10);
                    self.score += points;

                    // Explosion
                    for _ in 0..15 {
                        self.particles.push(Particle::new(
                            enemy.x + ENEMY_WIDTH / 2.0,
                            enemy.y + ENEMY_HEIGHT / 2.0,
                            enemy.color,
                        ));
                    }
                    break;
                }
            }
        }

        // Enemy bullets vs player
        if !self.player.invincible {
            for bullet in &mut self.enemy_bullets {
                if !bullet.alive {
                    continue;
                }

                if bullet.x < px + pw
                    && bullet.x + BULLET_WIDTH > px
                    && bullet.y < py + ph
                    && bullet.y + BULLET_HEIGHT > py
                {
                    bullet.alive = false;
                    self.lives = self.lives.saturating_sub(1);
                    self.player.hit();
                }
            }

            // Enemies vs player (collision)
            for enemy in &mut self.enemies {
                if !enemy.alive || !enemy.diving {
                    continue;
                }

                if enemy.x < px + pw
                    && enemy.x + ENEMY_WIDTH > px
                    && enemy.y < py + ph
                    && enemy.y + ENEMY_HEIGHT > py
                {
                    enemy.alive = false;
                    self.lives = self.lives.saturating_sub(1);
                    self.player.hit();
                }
            }
        }
    }

    fn render_game_frame(&mut self) {
        let width = GAME_WIDTH as usize;
        let height = GAME_HEIGHT as usize;
        let size = width * height * 4;

        // Use scratch buffer for rendering
        self.scratch_buffer.clear();
        self.scratch_buffer.resize(size, 0);

        // Clear with space background (very dark blue-black)
        for pixel in self.scratch_buffer.chunks_exact_mut(4) {
            pixel[0] = 5; // R
            pixel[1] = 5; // G
            pixel[2] = 15; // B
            pixel[3] = 255; // A
        }

        // Draw stars
        for star in &self.stars {
            let x = star.x as usize;
            let y = star.y as usize;
            let star_size = star.size as usize;
            if x < width && y < height && star_size > 0 {
                let alpha = star.brightness;
                // Draw star with size
                for dy in 0..star_size {
                    let sy = y.saturating_add(dy);
                    if sy >= height {
                        continue;
                    }
                    for dx in 0..star_size {
                        let sx = x.saturating_add(dx);
                        if sx >= width {
                            continue;
                        }
                        let offset = (sy * width + sx) * 4;
                        if offset + 3 < self.scratch_buffer.len() {
                            self.scratch_buffer[offset] = alpha;
                            self.scratch_buffer[offset + 1] = alpha;
                            self.scratch_buffer[offset + 2] = alpha + 20;
                            self.scratch_buffer[offset + 3] = 255;
                        }
                    }
                }
            }
        }

        // Draw enemies
        for enemy in &self.enemies {
            if !enemy.alive {
                continue;
            }

            let color = enemy.color;
            let r = ((color >> 16) & 0xFF) as u8;
            let g = ((color >> 8) & 0xFF) as u8;
            let b = (color & 0xFF) as u8;

            let ex = enemy.x as usize;
            let ey = enemy.y as usize;
            let ew = ENEMY_WIDTH as usize;
            let eh = ENEMY_HEIGHT as usize;

            // Draw enemy shape (simplified butterfly/bug shape)
            for y in ey.saturating_sub(eh / 2)..(ey + eh / 2).min(height) {
                for x in ex.saturating_sub(ew / 2)..(ex + ew / 2).min(width) {
                    let offset = (y * width + x) * 4;
                    if offset + 3 < self.scratch_buffer.len() {
                        self.scratch_buffer[offset] = r;
                        self.scratch_buffer[offset + 1] = g;
                        self.scratch_buffer[offset + 2] = b;
                        self.scratch_buffer[offset + 3] = 255;
                    }
                }
            }
        }

        // Draw player ship
        let (px, py, pw, ph) = self.player.get_rect();

        // Flicker if invincible
        if !self.player.invincible
            || ((self.player.invincible_timer * 10.0) as u32).is_multiple_of(2)
        {
            let ship_color = 0x00AAFF; // Cyan blue
            let sr = ((ship_color >> 16) & 0xFF) as u8;
            let sg = ((ship_color >> 8) & 0xFF) as u8;
            let sb = (ship_color & 0xFF) as u8;

            for y in py as usize..(py + ph) as usize {
                for x in px as usize..(px + pw) as usize {
                    if x >= width || y >= height {
                        continue;
                    }

                    // Draw ship shape (pointed triangle-ish)
                    let center_x = px + pw / 2.0;
                    let rel_x = x as f32 - center_x;
                    let rel_y = y as f32 - py;

                    // Narrow towards front (top)
                    let max_width_at_y = pw * (1.0 - rel_y / ph * 0.7);

                    if rel_x.abs() < max_width_at_y / 2.0 {
                        let offset = (y * width + x) * 4;
                        if offset + 3 < self.scratch_buffer.len() {
                            self.scratch_buffer[offset] = sr;
                            self.scratch_buffer[offset + 1] = sg;
                            self.scratch_buffer[offset + 2] = sb;
                            self.scratch_buffer[offset + 3] = 255;
                        }
                    }
                }
            }

            // Engine glow
            let glow_color = 0xFF6600;
            let gr = ((glow_color >> 16) & 0xFF) as u8;
            let gg = ((glow_color >> 8) & 0xFF) as u8;
            let gb = (glow_color & 0xFF) as u8;

            let engine_y_start = (py + ph) as usize;
            let engine_y_end = (py + ph + 8.0) as usize;
            let engine_x_start = (px + pw / 4.0) as usize;
            let engine_x_end = (px + pw * 3.0 / 4.0) as usize;

            for y in engine_y_start..engine_y_end.min(height) {
                for x in engine_x_start..engine_x_end.min(width) {
                    let offset = (y * width + x) * 4;
                    if offset + 3 < self.scratch_buffer.len() {
                        self.scratch_buffer[offset] = gr;
                        self.scratch_buffer[offset + 1] = gg;
                        self.scratch_buffer[offset + 2] = gb;
                        self.scratch_buffer[offset + 3] = 255;
                    }
                }
            }
        }

        // Draw player bullets
        for bullet in &self.bullets {
            if !bullet.alive {
                continue;
            }

            let bx = bullet.x as usize;
            let by = bullet.y as usize;

            for y in by.saturating_sub(BULLET_HEIGHT as usize / 2)
                ..(by + BULLET_HEIGHT as usize).min(height)
            {
                for x in bx.saturating_sub(BULLET_WIDTH as usize / 2)
                    ..(bx + BULLET_WIDTH as usize).min(width)
                {
                    let offset = (y * width + x) * 4;
                    if offset + 3 < self.scratch_buffer.len() {
                        self.scratch_buffer[offset] = 255; // R
                        self.scratch_buffer[offset + 1] = 255; // G
                        self.scratch_buffer[offset + 2] = 100; // B
                        self.scratch_buffer[offset + 3] = 255; // A
                    }
                }
            }
        }

        // Draw enemy bullets
        for bullet in &self.enemy_bullets {
            if !bullet.alive {
                continue;
            }

            let bx = bullet.x as usize;
            let by = bullet.y as usize;

            for y in by.saturating_sub(BULLET_HEIGHT as usize / 2)
                ..(by + BULLET_HEIGHT as usize).min(height)
            {
                for x in bx.saturating_sub(BULLET_WIDTH as usize / 2)
                    ..(bx + BULLET_WIDTH as usize).min(width)
                {
                    let offset = (y * width + x) * 4;
                    if offset + 3 < self.scratch_buffer.len() {
                        self.scratch_buffer[offset] = 255; // R
                        self.scratch_buffer[offset + 1] = 100; // G
                        self.scratch_buffer[offset + 2] = 100; // B
                        self.scratch_buffer[offset + 3] = 255; // A
                    }
                }
            }
        }

        // Draw particles
        for particle in &self.particles {
            let color = particle.color;
            let r = ((color >> 16) & 0xFF) as u8;
            let g = ((color >> 8) & 0xFF) as u8;
            let b = (color & 0xFF) as u8;
            let alpha = (particle.life / particle.max_life * 255.0) as u8;

            let px = particle.x as usize;
            let py = particle.y as usize;
            let psize = particle.size as usize;

            for y in py.saturating_sub(psize / 2)..(py + psize / 2).min(height) {
                for x in px.saturating_sub(psize / 2)..(px + psize / 2).min(width) {
                    let offset = (y * width + x) * 4;
                    if offset + 3 < self.scratch_buffer.len() {
                        self.scratch_buffer[offset] = r;
                        self.scratch_buffer[offset + 1] = g;
                        self.scratch_buffer[offset + 2] = b;
                        self.scratch_buffer[offset + 3] = alpha;
                    }
                }
            }
        }

        // Swap scratch buffer into frame buffer
        let new_buffer = std::mem::take(&mut self.scratch_buffer);
        self.frame_buffer = Arc::new(new_buffer);
        self.scratch_buffer = Vec::with_capacity(size);
    }
}

// ============================================================================
// PLUGIN TRAIT IMPLEMENTATION
// ============================================================================

impl Plugin for GalagaPlugin {
    fn id(&self) -> &str {
        "galaga"
    }

    fn name(&self) -> &str {
        "Galaga"
    }

    fn display_name(&self) -> String {
        format!(
            "Galaga (Stage {} | Score: {} | Lives: {})",
            self.stage, self.score, self.lives
        )
    }

    fn render_mode(&self) -> PluginRenderMode {
        match self.state {
            GameState::Menu
            | GameState::Paused
            | GameState::GameOver
            | GameState::LevelComplete
            | GameState::StageClear => PluginRenderMode::Text,
            GameState::Playing => PluginRenderMode::KittyGraphics,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer, _ctx: &PluginContext) {
        match self.state {
            GameState::Menu => {
                let title = Span::styled(
                    "🚀 GALAGA 🚀",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(ratatui::style::Modifier::BOLD),
                );

                let instructions = vec![
                    Line::from(""),
                    Line::from("Controls:"),
                    Line::from("  A/D or ←/→ : Move ship"),
                    Line::from("  SPACE       : Fire"),
                    Line::from("  P           : Pause game"),
                    Line::from("  R           : Restart"),
                    Line::from(""),
                    Line::from(Span::styled(
                        "Press SPACE to start",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(ratatui::style::Modifier::BOLD),
                    )),
                ];

                let paragraph = Paragraph::new(vec![Line::from(title)])
                    .alignment(ratatui::layout::Alignment::Center)
                    .block(
                        ratatui::widgets::Block::default()
                            .borders(ratatui::widgets::Borders::ALL)
                            .border_type(ratatui::widgets::BorderType::Rounded)
                            .border_style(Style::default().fg(Color::Cyan)),
                    );

                paragraph.render(area, buf);

                let instructions_area = Rect {
                    x: area.x,
                    y: area.y + 6,
                    width: area.width,
                    height: area.height.saturating_sub(6),
                };

                let instructions_paragraph =
                    Paragraph::new(instructions).alignment(ratatui::layout::Alignment::Center);

                instructions_paragraph.render(instructions_area, buf);
            }
            GameState::Paused => {
                let text = vec![
                    Line::from(""),
                    Line::from(Span::styled(
                        "⏸️ PAUSED",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(ratatui::style::Modifier::BOLD),
                    )),
                    Line::from(""),
                    Line::from("Press P to resume"),
                ];

                let paragraph = Paragraph::new(text)
                    .alignment(ratatui::layout::Alignment::Center)
                    .block(
                        ratatui::widgets::Block::default()
                            .borders(ratatui::widgets::Borders::ALL)
                            .border_type(ratatui::widgets::BorderType::Rounded),
                    );

                paragraph.render(area, buf);
            }
            GameState::GameOver => {
                let text = vec![
                    Line::from(""),
                    Line::from(Span::styled(
                        "💀 GAME OVER 💀",
                        Style::default()
                            .fg(Color::Red)
                            .add_modifier(ratatui::style::Modifier::BOLD),
                    )),
                    Line::from(""),
                    Line::from(format!("Final Score: {}", self.score)),
                    Line::from(format!("Stage Reached: {}", self.stage)),
                    Line::from(format!("High Score: {}", self.high_score)),
                    Line::from(""),
                    Line::from(Span::styled(
                        "Press R to restart or Q to quit",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(ratatui::style::Modifier::BOLD),
                    )),
                ];

                let paragraph = Paragraph::new(text)
                    .alignment(ratatui::layout::Alignment::Center)
                    .block(
                        ratatui::widgets::Block::default()
                            .borders(ratatui::widgets::Borders::ALL)
                            .border_type(ratatui::widgets::BorderType::Rounded),
                    );

                paragraph.render(area, buf);
            }
            GameState::LevelComplete => {
                let text = vec![
                    Line::from(""),
                    Line::from(Span::styled(
                        "🎉 STAGE CLEAR! 🎉",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(ratatui::style::Modifier::BOLD),
                    )),
                    Line::from(""),
                    Line::from(format!("Stage {} cleared!", self.stage)),
                    Line::from(format!("Current Score: {}", self.score)),
                    Line::from(""),
                    Line::from(Span::styled(
                        "Press SPACE for next stage",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(ratatui::style::Modifier::BOLD),
                    )),
                ];

                let paragraph = Paragraph::new(text)
                    .alignment(ratatui::layout::Alignment::Center)
                    .block(
                        ratatui::widgets::Block::default()
                            .borders(ratatui::widgets::Borders::ALL)
                            .border_type(ratatui::widgets::BorderType::Rounded),
                    );

                paragraph.render(area, buf);
            }
            GameState::StageClear => {
                let text = vec![
                    Line::from(""),
                    Line::from(Span::styled(
                        "🏆 ALL STAGES CLEARED! 🏆",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(ratatui::style::Modifier::BOLD),
                    )),
                    Line::from(""),
                    Line::from(format!("Final Score: {}", self.score)),
                    Line::from(format!("High Score: {}", self.high_score)),
                    Line::from(""),
                    Line::from(Span::styled(
                        "Press R to play again",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(ratatui::style::Modifier::BOLD),
                    )),
                ];

                let paragraph = Paragraph::new(text)
                    .alignment(ratatui::layout::Alignment::Center)
                    .block(
                        ratatui::widgets::Block::default()
                            .borders(ratatui::widgets::Borders::ALL)
                            .border_type(ratatui::widgets::BorderType::Rounded),
                    );

                paragraph.render(area, buf);
            }
            GameState::Playing => {
                // KittyGraphics mode - render_game_frame() is used instead
            }
        }
    }

    fn render_frame(&mut self, _width: u32, _height: u32) -> Option<PluginFrame> {
        if self.state != GameState::Playing {
            return None;
        }

        if self.frame_ready {
            self.render_game_frame();
            self.frame_ready = false;

            // Zero-copy: use Arc::clone()
            Some(PluginFrame::from_arc(
                Arc::clone(&self.frame_buffer),
                GAME_WIDTH,
                GAME_HEIGHT,
            ))
        } else {
            None
        }
    }

    fn handle_event(&mut self, event: &Event, _area: Rect) -> PluginEventResult {
        if let Event::Key(KeyEvent {
            code,
            modifiers,
            kind,
            ..
        }) = event
        {
            let no_modifiers = *modifiers == KeyModifiers::NONE;
            let is_press = *kind == KeyEventKind::Press || *kind == KeyEventKind::Repeat;
            let is_release = *kind == KeyEventKind::Release;

            // Global quit - let parent handle
            if matches!(code, KeyCode::Char('q') | KeyCode::Char('Q')) {
                return PluginEventResult::Ignored;
            }

            // Movement and fire keys - handle press AND release
            if self.state == GameState::Playing {
                match code {
                    KeyCode::Left | KeyCode::Char('a') | KeyCode::Char('A') => {
                        if is_press {
                            self.keys.left = true;
                        } else if is_release {
                            self.keys.left = false;
                        }
                        return PluginEventResult::Consumed;
                    }
                    KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('D') => {
                        if is_press {
                            self.keys.right = true;
                        } else if is_release {
                            self.keys.right = false;
                        }
                        return PluginEventResult::Consumed;
                    }
                    KeyCode::Char(' ') => {
                        if is_press {
                            self.keys.fire = true;
                        } else if is_release {
                            self.keys.fire = false;
                        }
                        return PluginEventResult::Consumed;
                    }
                    _ => {}
                }
            }

            // Only handle press events for other actions
            if !is_press {
                return PluginEventResult::Ignored;
            }

            match (self.state, code, no_modifiers) {
                // Menu state
                (GameState::Menu, KeyCode::Char(' '), true) => {
                    self.reset_game();
                    return PluginEventResult::Consumed;
                }

                // Playing state - other controls
                (GameState::Playing, KeyCode::Char('p') | KeyCode::Char('P'), true) => {
                    self.state = GameState::Paused;
                    return PluginEventResult::Consumed;
                }
                (GameState::Playing, KeyCode::Char('r') | KeyCode::Char('R'), true) => {
                    self.reset_game();
                    return PluginEventResult::Consumed;
                }

                // Paused state
                (GameState::Paused, KeyCode::Char('p') | KeyCode::Char('P'), true) => {
                    self.state = GameState::Playing;
                    return PluginEventResult::Consumed;
                }

                // Game Over state
                (GameState::GameOver, KeyCode::Char('r') | KeyCode::Char('R'), true) => {
                    self.reset_game();
                    return PluginEventResult::Consumed;
                }

                // Level Complete state
                (GameState::LevelComplete, KeyCode::Char(' '), true) => {
                    self.stage += 1;
                    self.generate_enemy_grid(self.stage);
                    self.load_stage();
                    self.state = GameState::Playing;
                    return PluginEventResult::Consumed;
                }

                // Stage Clear state
                (GameState::StageClear, KeyCode::Char('r') | KeyCode::Char('R'), true) => {
                    self.reset_game();
                    return PluginEventResult::Consumed;
                }

                _ => {}
            }
        }

        PluginEventResult::Ignored
    }

    fn tick(&mut self) -> bool {
        // Fixed time step of ~60fps (16.67ms)
        const DT: f32 = 1.0 / 60.0;
        self.update(DT);
        self.frame_ready
    }

    fn on_activate(&mut self) {
        // Reset input state
        self.keys = KeyState::default();
    }

    fn on_deactivate(&mut self) {
        // Pause game when switching away
        if self.state == GameState::Playing {
            self.state = GameState::Paused;
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Default for GalagaPlugin {
    fn default() -> Self {
        Self::new()
    }
}
