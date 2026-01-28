//! Pew-Pew Game Plugin
//!
//! A small Galaga-style shooter for the plugin window.
//!
//! Minimal spec (initial):
//! - Horizontal-only ship movement (A/D + ←/→)
//! - Space to shoot
//! - Level-based progression by clearing enemies
//! - 60fps fixed timestep
//! - Kitty graphics rendering while playing; text screens otherwise
//!
//! TODO: Work in progress - clippy warnings suppressed for now
#![allow(dead_code, clippy::collapsible_if, clippy::comparison_chain)]

use rand::prelude::*;
use std::any::Any;
use std::sync::Arc;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Paragraph, Widget},
};

use super::{Plugin, PluginContext, PluginEventResult, PluginFrame, PluginRenderMode};

// ============================================================================
// CONSTANTS & CONFIGURATION
// ============================================================================

/// Internal game resolution (scaled by Kitty placement)
pub const GAME_WIDTH: u32 = 640;
pub const GAME_HEIGHT: u32 = 480;

const INITIAL_LIVES: u8 = 3;

// Player
const PLAYER_WIDTH: f32 = 44.0;
const PLAYER_HEIGHT: f32 = 22.0;
const PLAYER_Y: f32 = 430.0;
const PLAYER_SPEED: f32 = 520.0;

// Enemy formation constants
const ENEMY_COLS: usize = 10;
const ENEMY_ROWS: usize = 4;
const ENEMY_WIDTH: f32 = 34.0;
const ENEMY_HEIGHT: f32 = 22.0;
const ENEMY_PAD_X: f32 = 14.0;
const ENEMY_PAD_Y: f32 = 14.0;
const ENEMY_TOP: f32 = 70.0;
const ENEMY_LEFT: f32 = 80.0;

// Formation movement
const FORMATION_SPEED_BASE: f32 = 60.0;
const FORMATION_DESCEND_AMOUNT: f32 = 20.0;

// ============================================================================
// ENEMY TYPES
// ============================================================================

/// Enemy type with different colors and point values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyType {
    /// Butterfly - fast, medium points (red)
    Butterfly,
    /// Boss - slow, high points (orange)
    Boss,
    /// Drone - slow, low points (green)
    Drone,
}

impl EnemyType {
    /// Get color for this enemy type (RGBA hex)
    pub fn color(self) -> u32 {
        match self {
            EnemyType::Butterfly => 0xFF5050, // Red
            EnemyType::Boss => 0xFFC850,      // Orange
            EnemyType::Drone => 0x78DC78,     // Green
        }
    }

    /// Point value for destroying this enemy
    pub fn points(self) -> u32 {
        match self {
            EnemyType::Butterfly => 80,
            EnemyType::Boss => 150,
            EnemyType::Drone => 50,
        }
    }
}

/// Individual enemy entity
#[derive(Debug, Clone)]
pub struct Enemy {
    /// Enemy type
    pub enemy_type: EnemyType,
    /// Position
    pub x: f32,
    pub y: f32,
    /// Velocity (for diving enemies)
    pub vx: f32,
    pub vy: f32,
    /// Formation grid position
    pub col: usize,
    pub row: usize,
    /// Alive status
    pub alive: bool,
    /// Diving state (true when leaving formation to attack)
    pub is_diving: bool,
}

impl Enemy {
    /// Create enemy in formation
    pub fn new(col: usize, row: usize) -> Self {
        let x = ENEMY_LEFT + col as f32 * (ENEMY_WIDTH + ENEMY_PAD_X);
        let y = ENEMY_TOP + row as f32 * (ENEMY_HEIGHT + ENEMY_PAD_Y);

        let enemy_type = match row {
            0 => EnemyType::Butterfly,
            1 | 2 => EnemyType::Boss,
            _ => EnemyType::Drone,
        };

        Self {
            enemy_type,
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            col,
            row,
            alive: true,
            is_diving: false,
        }
    }

    /// Check if enemy is in valid formation position
    fn in_formation(&self) -> bool {
        !self.is_diving && self.alive
    }

    /// Start a diving attack
    pub fn start_dive(&mut self, target_x: f32) {
        if !self.in_formation() {
            return;
        }

        self.is_diving = true;

        // Calculate velocity to aim at target
        let dx = target_x - self.x;

        // Tweak for different dive patterns
        match self.enemy_type {
            EnemyType::Butterfly => {
                // Fast, straight down with slight angle
                self.vx = dx * 0.3;
                self.vy = 300.0;
            }
            EnemyType::Boss => {
                // Slow, curved dive
                self.vx = dx * 0.2;
                self.vy = 200.0;
            }
            EnemyType::Drone => {
                // Medium speed, angled
                self.vx = dx * 0.4 + (rand::random::<f32>() - 0.5) * 50.0;
                self.vy = 250.0;
            }
        }
    }
}

/// Enemy formation manager
#[derive(Debug, Clone)]
pub struct EnemyFormation {
    /// All enemies in the formation
    pub enemies: Vec<Enemy>,
    /// Formation direction (1 = right, -1 = left)
    direction: f32,
    /// Current formation left bound
    formation_left: f32,
    /// Current formation right bound
    formation_right: f32,
    /// Formation speed (increases as enemies are destroyed)
    speed: f32,
    /// Formation vertical offset
    formation_y: f32,
    /// Minimum X position for formation
    min_x: f32,
    /// Maximum X position for formation
    max_x: f32,
}

impl EnemyFormation {
    /// Create new formation for a level
    pub fn new(level: u8) -> Self {
        let mut enemies = Vec::with_capacity(ENEMY_COLS * ENEMY_ROWS);

        for row in 0..ENEMY_ROWS {
            for col in 0..ENEMY_COLS {
                // Skip some enemies on higher levels for variety
                if level > 2 && (col + row) % 3 == 0 {
                    continue;
                }
                enemies.push(Enemy::new(col, row));
            }
        }

        let min_x = ENEMY_LEFT;
        let max_x =
            ENEMY_LEFT + (ENEMY_COLS as f32 - 1.0) * (ENEMY_WIDTH + ENEMY_PAD_X) + ENEMY_WIDTH;

        Self {
            enemies,
            direction: 1.0,
            formation_left: min_x,
            formation_right: max_x,
            speed: FORMATION_SPEED_BASE * (1.0 + level as f32 * 0.1),
            formation_y: 0.0,
            min_x,
            max_x,
        }
    }

    /// Update formation movement
    pub fn update(&mut self, dt: f32) {
        // Move formation horizontally
        let move_amount = self.direction * self.speed * dt;
        self.formation_left += move_amount;
        self.formation_right += move_amount;

        // Check bounds and reverse direction
        if self.formation_left <= 0.0 {
            self.direction = 1.0;
            self.formation_y += FORMATION_DESCEND_AMOUNT;
        } else if self.formation_right >= GAME_WIDTH as f32 {
            self.direction = -1.0;
            self.formation_y += FORMATION_DESCEND_AMOUNT;
        }

        // Update all formation enemies
        for enemy in &mut self.enemies {
            if enemy.in_formation() {
                enemy.x = ENEMY_LEFT
                    + enemy.col as f32 * (ENEMY_WIDTH + ENEMY_PAD_X)
                    + self.formation_left
                    - self.min_x;
                enemy.y =
                    ENEMY_TOP + enemy.row as f32 * (ENEMY_HEIGHT + ENEMY_PAD_Y) + self.formation_y;
            } else if enemy.is_diving {
                // Update diving enemies
                enemy.x += enemy.vx * dt;
                enemy.y += enemy.vy * dt;

                // Remove if off screen
                if enemy.y > GAME_HEIGHT as f32
                    || enemy.x < -50.0
                    || enemy.x > GAME_WIDTH as f32 + 50.0
                {
                    enemy.alive = false;
                }
            }
        }

        // Clean up dead enemies
        self.enemies.retain(|e| e.alive);

        // Increase speed as enemies are destroyed
        let total_enemies = ENEMY_COLS * ENEMY_ROWS;
        let alive_count = self
            .enemies
            .iter()
            .filter(|e| e.alive && !e.is_diving)
            .count();
        if alive_count > 0 {
            let destruction_ratio = 1.0 - (alive_count as f32 / total_enemies as f32);
            self.speed = FORMATION_SPEED_BASE * (1.0 + destruction_ratio * 2.0);
        }
    }

    /// Get count of active (non-diving) enemies
    pub fn active_count(&self) -> usize {
        self.enemies
            .iter()
            .filter(|e| e.alive && !e.is_diving)
            .count()
    }

    /// Check if formation is cleared
    pub fn is_cleared(&self) -> bool {
        self.enemies.is_empty()
    }

    /// Trigger a random enemy to dive attack
    /// Returns true if an enemy was triggered
    pub fn trigger_dive(&mut self, target_x: f32) -> bool {
        // Get enemies in formation
        let in_formation: Vec<usize> = self
            .enemies
            .iter()
            .enumerate()
            .filter(|(_, e)| e.in_formation())
            .map(|(i, _)| i)
            .collect();

        if in_formation.is_empty() {
            return false;
        }

        // Random chance to trigger dive based on level
        let chance = 0.02 + (self.enemies.len() as f32 / 100.0);
        if rand::random::<f32>() > chance {
            return false;
        }

        // Pick random enemy to dive
        if let Some(&idx) = in_formation.iter().choose(&mut rand::thread_rng()) {
            self.enemies[idx].start_dive(target_x);
            return true;
        }

        false
    }
}

// ============================================================================
// PROJECTILE SYSTEM
// ============================================================================

/// Weapon power-up types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaponType {
    /// Single shot (default)
    Single,
    /// Double shot (two parallel bullets)
    Double,
    /// Spread shot (three bullets in a fan)
    Spread,
    /// Fast shot (faster fire rate)
    Fast,
}

/// Projectile type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectileType {
    /// Player bullet (moves up)
    PlayerBullet,
    /// Enemy bullet (moves down)
    EnemyBullet,
}

/// Individual projectile
#[derive(Debug, Clone)]
pub struct Projectile {
    pub projectile_type: ProjectileType,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub vx: f32,
    pub vy: f32,
    pub alive: bool,
}

impl Projectile {
    /// Create player bullet
    pub fn new_player(x: f32, y: f32) -> Self {
        Self {
            projectile_type: ProjectileType::PlayerBullet,
            x,
            y,
            width: 4.0,
            height: 12.0,
            vx: 0.0,
            vy: -800.0,
            alive: true,
        }
    }

    /// Create player bullet with custom velocity
    pub fn new_player_with_vel(x: f32, y: f32, vx: f32, vy: f32) -> Self {
        Self {
            projectile_type: ProjectileType::PlayerBullet,
            x,
            y,
            width: 4.0,
            height: 12.0,
            vx,
            vy,
            alive: true,
        }
    }

    /// Create enemy bullet
    pub fn new_enemy(x: f32, y: f32) -> Self {
        Self {
            projectile_type: ProjectileType::EnemyBullet,
            x,
            y,
            width: 4.0,
            height: 12.0,
            vx: 0.0,
            vy: 300.0,
            alive: true,
        }
    }

    /// Update projectile position
    pub fn update(&mut self, dt: f32) {
        self.x += self.vx * dt;
        self.y += self.vy * dt;

        // Remove if off screen
        if self.y < -50.0 || self.y > GAME_HEIGHT as f32 + 50.0 {
            self.alive = false;
        }
    }

    /// Check collision with a rectangle
    pub fn hits(&self, ox: f32, oy: f32, ow: f32, oh: f32) -> bool {
        if !self.alive {
            return false;
        }

        self.x < ox + ow
            && self.x + self.width > ox
            && self.y < oy + oh
            && self.y + self.height > oy
    }
}

/// Projectile manager
#[derive(Debug, Clone)]
pub struct ProjectileManager {
    pub projectiles: Vec<Projectile>,
    /// Cooldown timer for player shooting
    shoot_cooldown: f32,
    /// Time between shots
    shoot_delay: f32,
    /// Current weapon type
    weapon_type: WeaponType,
    /// Weapon level (affects fire rate and power)
    weapon_level: u8,
}

impl ProjectileManager {
    /// Create new projectile manager
    pub fn new() -> Self {
        Self {
            projectiles: Vec::new(),
            shoot_cooldown: 0.0,
            shoot_delay: 0.15, // ~6 shots per second
            weapon_type: WeaponType::Single,
            weapon_level: 1,
        }
    }

    /// Update cooldown
    pub fn update(&mut self, dt: f32) {
        if self.shoot_cooldown > 0.0 {
            self.shoot_cooldown -= dt;
        }

        // Update all projectiles
        for proj in &mut self.projectiles {
            proj.update(dt);
        }

        // Remove dead projectiles
        self.projectiles.retain(|p| p.alive);
    }

    /// Check if player can shoot
    pub fn can_shoot(&self) -> bool {
        self.shoot_cooldown <= 0.0
    }

    /// Get current weapon type
    pub fn weapon_type(&self) -> WeaponType {
        self.weapon_type
    }

    /// Get weapon level
    pub fn weapon_level(&self) -> u8 {
        self.weapon_level
    }

    /// Upgrade weapon
    pub fn upgrade_weapon(&mut self) {
        self.weapon_level = (self.weapon_level + 1).min(5);
        self.shoot_delay = (self.shoot_delay * 0.9).max(0.08); // Faster fire rate
    }

    /// Set weapon type
    pub fn set_weapon(&mut self, weapon: WeaponType) {
        self.weapon_type = weapon;
    }

    /// Fire player bullet based on current weapon
    pub fn fire_player(&mut self, x: f32, y: f32) {
        if !self.can_shoot() {
            return;
        }

        match self.weapon_type {
            WeaponType::Single => {
                self.projectiles.push(Projectile::new_player(x, y));
            }
            WeaponType::Double => {
                // Two parallel bullets
                self.projectiles
                    .push(Projectile::new_player(x - 8.0, y));
                self.projectiles
                    .push(Projectile::new_player(x + 4.0, y));
            }
            WeaponType::Spread => {
                // Three bullets in a fan
                self.projectiles.push(Projectile::new_player(x, y));
                self.projectiles
                    .push(Projectile::new_player_with_vel(x - 10.0, y, -100.0, -750.0));
                self.projectiles
                    .push(Projectile::new_player_with_vel(x + 6.0, y, 100.0, -750.0));
            }
            WeaponType::Fast => {
                // Single fast bullet
                self.projectiles.push(Projectile::new_player(x, y));
            }
        }

        self.shoot_cooldown = self.shoot_delay;
    }

    /// Fire enemy bullet
    pub fn fire_enemy(&mut self, x: f32, y: f32) {
        self.projectiles.push(Projectile::new_enemy(x, y));
    }

    /// Get player bullets for collision checking
    pub fn player_bullets(&self) -> impl Iterator<Item = &Projectile> {
        self.projectiles
            .iter()
            .filter(|p| p.projectile_type == ProjectileType::PlayerBullet)
    }

    /// Get enemy bullets for collision checking
    pub fn enemy_bullets(&self) -> impl Iterator<Item = &Projectile> {
        self.projectiles
            .iter()
            .filter(|p| p.projectile_type == ProjectileType::EnemyBullet)
    }
}

// ============================================================================
// GAME STATE
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameState {
    Menu,
    Playing,
    Paused,
    #[allow(dead_code)]
    GameOver,
    #[allow(dead_code)]
    LevelComplete,
}

#[derive(Debug, Clone, Default)]
struct KeyState {
    left: bool,
    right: bool,
}

// ============================================================================
// MAIN PLUGIN STRUCT
// ============================================================================

pub struct PewPewPlugin {
    // State
    state: GameState,

    // Player state
    player_x: f32,

    // Enemy formation
    formation: EnemyFormation,

    // Projectiles
    projectiles: ProjectileManager,

    // Score/lives/level
    score: u32,
    lives: u8,
    level: u8,
    high_score: u32,

    // Input
    keys: KeyState,

    // Rendering
    frame_buffer: Arc<Vec<u8>>,
    scratch_buffer: Vec<u8>,
    frame_ready: bool,
}

impl PewPewPlugin {
    pub fn new() -> Self {
        let size = (GAME_WIDTH * GAME_HEIGHT * 4) as usize;
        let player_x = GAME_WIDTH as f32 / 2.0 - PLAYER_WIDTH / 2.0;
        let formation = EnemyFormation::new(1);

        Self {
            state: GameState::Menu,
            player_x,
            formation,
            projectiles: ProjectileManager::new(),
            score: 0,
            lives: INITIAL_LIVES,
            level: 1,
            high_score: 0,
            keys: KeyState::default(),
            frame_buffer: Arc::new(vec![0; size]),
            scratch_buffer: vec![0; size],
            frame_ready: false,
        }
    }

    fn load_level(&mut self) {
        self.player_x = GAME_WIDTH as f32 / 2.0 - PLAYER_WIDTH / 2.0;
        self.formation = EnemyFormation::new(self.level);
        self.projectiles = ProjectileManager::new();
        self.keys = KeyState::default();
        self.frame_ready = true;
    }

    fn reset_game(&mut self) {
        self.score = 0;
        self.lives = INITIAL_LIVES;
        self.level = 1;
        self.load_level();
        self.state = GameState::Playing;
    }

    fn check_collisions(&mut self) {
        // Player bullets vs enemies
        for bullet in self.projectiles.player_bullets() {
            for enemy in &mut self.formation.enemies {
                if enemy.alive && !enemy.is_diving {
                    if bullet.hits(enemy.x, enemy.y, ENEMY_WIDTH, ENEMY_HEIGHT) {
                        enemy.alive = false;
                        // Mark bullet as dead (need mutable access)
                        // We'll clean up in the next update
                        break;
                    }
                }
            }
        }

        // Remove dead bullets that hit something
        self.projectiles.projectiles.retain(|p| {
            if p.projectile_type == ProjectileType::PlayerBullet && p.alive {
                // Check if it hit anything
                for enemy in &self.formation.enemies {
                    if enemy.alive && !enemy.is_diving {
                        if p.hits(enemy.x, enemy.y, ENEMY_WIDTH, ENEMY_HEIGHT) {
                            return false;
                        }
                    }
                }
            }
            p.alive
        });
    }

    fn update(&mut self, dt: f32) {
        if self.state != GameState::Playing {
            return;
        }

        // Update player
        let mut direction = 0.0;
        if self.keys.left {
            direction -= 1.0;
        }
        if self.keys.right {
            direction += 1.0;
        }

        self.player_x += direction * PLAYER_SPEED * dt;
        self.player_x = self.player_x.clamp(0.0, GAME_WIDTH as f32 - PLAYER_WIDTH);

        // Update enemy formation
        self.formation.update(dt);

        // Trigger random enemy dives
        let player_center = self.player_x + PLAYER_WIDTH / 2.0;
        self.formation.trigger_dive(player_center);

        // Update projectiles
        self.projectiles.update(dt);

        // Check collisions
        self.check_collisions();

        // Check level complete
        if self.formation.is_cleared() {
            self.state = GameState::LevelComplete;
        }

        self.frame_ready = true;
    }

    fn render_game_frame(&mut self) {
        let width = GAME_WIDTH as usize;
        let height = GAME_HEIGHT as usize;
        let size = width * height * 4;

        // Fill background (RGBA)
        for pixel in self.scratch_buffer.chunks_exact_mut(4) {
            pixel[0] = 10; // R
            pixel[1] = 10; // G
            pixel[2] = 18; // B
            pixel[3] = 255; // A
        }

        // Draw enemies from formation
        for enemy in &self.formation.enemies {
            if !enemy.alive {
                continue;
            }

            let color = enemy.enemy_type.color();
            let r = ((color >> 16) & 0xFF) as u8;
            let g = ((color >> 8) & 0xFF) as u8;
            let b = (color & 0xFF) as u8;

            let x0 = enemy.x as usize;
            let y0 = enemy.y as usize;
            let x1 = (enemy.x + ENEMY_WIDTH) as usize;
            let y1 = (enemy.y + ENEMY_HEIGHT) as usize;

            for py in y0..y1.min(height) {
                for px in x0..x1.min(width) {
                    let offset = (py * width + px) * 4;
                    self.scratch_buffer[offset] = r;
                    self.scratch_buffer[offset + 1] = g;
                    self.scratch_buffer[offset + 2] = b;
                    self.scratch_buffer[offset + 3] = 255;
                }
            }

            // Add eyes to enemies for visual distinction
            let eye_color = 0xFFFFFF;
            let eye_r = ((eye_color >> 16) & 0xFF) as u8;
            let eye_g = ((eye_color >> 8) & 0xFF) as u8;
            let eye_b = (eye_color & 0xFF) as u8;

            let eye_y = (enemy.y + ENEMY_HEIGHT * 0.3) as usize;
            let left_eye_x = (enemy.x + ENEMY_WIDTH * 0.25) as usize;
            let right_eye_x = (enemy.x + ENEMY_WIDTH * 0.65) as usize;
            let eye_size = 4usize;

            for dy in 0..eye_size {
                for dx in 0..eye_size {
                    let ey = eye_y + dy;
                    let ex_l = left_eye_x + dx;
                    let ex_r = right_eye_x + dx;
                    if ey < height {
                        if ex_l < width {
                            let offset = (ey * width + ex_l) * 4;
                            self.scratch_buffer[offset] = eye_r;
                            self.scratch_buffer[offset + 1] = eye_g;
                            self.scratch_buffer[offset + 2] = eye_b;
                            self.scratch_buffer[offset + 3] = 255;
                        }
                        if ex_r < width {
                            let offset = (ey * width + ex_r) * 4;
                            self.scratch_buffer[offset] = eye_r;
                            self.scratch_buffer[offset + 1] = eye_g;
                            self.scratch_buffer[offset + 2] = eye_b;
                            self.scratch_buffer[offset + 3] = 255;
                        }
                    }
                }
            }
        }

        // Player ship
        let ship_color = (79u8, 195u8, 247u8); // light blue
        let x0 = self.player_x as usize;
        let y0 = PLAYER_Y as usize;
        let x1 = (self.player_x + PLAYER_WIDTH) as usize;
        let y1 = (PLAYER_Y + PLAYER_HEIGHT) as usize;

        for py in y0..y1.min(height) {
            for px in x0..x1.min(width) {
                let offset = (py * width + px) * 4;
                self.scratch_buffer[offset] = ship_color.0;
                self.scratch_buffer[offset + 1] = ship_color.1;
                self.scratch_buffer[offset + 2] = ship_color.2;
                self.scratch_buffer[offset + 3] = 255;
            }
        }

        // Simple cockpit highlight
        let cockpit_color = (255u8, 255u8, 255u8);
        let cx0 = (self.player_x + PLAYER_WIDTH / 2.0 - 6.0) as usize;
        let cy0 = (PLAYER_Y + 4.0) as usize;
        let cx1 = (self.player_x + PLAYER_WIDTH / 2.0 + 6.0) as usize;
        let cy1 = (PLAYER_Y + 12.0) as usize;

        for py in cy0..cy1.min(height) {
            for px in cx0..cx1.min(width) {
                let offset = (py * width + px) * 4;
                self.scratch_buffer[offset] = cockpit_color.0;
                self.scratch_buffer[offset + 1] = cockpit_color.1;
                self.scratch_buffer[offset + 2] = cockpit_color.2;
                self.scratch_buffer[offset + 3] = 255;
            }
        }

        // Draw projectiles
        for proj in &self.projectiles.projectiles {
            if !proj.alive {
                continue;
            }

            let (r, g, b) = match proj.projectile_type {
                ProjectileType::PlayerBullet => (255u8, 255u8, 100u8), // Yellow for player
                ProjectileType::EnemyBullet => (255u8, 100u8, 100u8), // Red for enemy
            };

            let x0 = proj.x as usize;
            let y0 = proj.y as usize;
            let x1 = (proj.x + proj.width) as usize;
            let y1 = (proj.y + proj.height) as usize;

            for py in y0..y1.min(height) {
                for px in x0..x1.min(width) {
                    let offset = (py * width + px) * 4;
                    self.scratch_buffer[offset] = r;
                    self.scratch_buffer[offset + 1] = g;
                    self.scratch_buffer[offset + 2] = b;
                    self.scratch_buffer[offset + 3] = 255;
                }
            }
        }

        // Swap scratch buffer into frame buffer wrapped in Arc
        let new_buffer = std::mem::replace(&mut self.scratch_buffer, vec![0; size]);
        self.frame_buffer = Arc::new(new_buffer);
    }
}

// ============================================================================
// PLUGIN TRAIT IMPLEMENTATION
// ============================================================================

impl Plugin for PewPewPlugin {
    fn id(&self) -> &str {
        "pew_pew"
    }

    fn name(&self) -> &str {
        "Pew-Pew"
    }

    fn display_name(&self) -> String {
        format!(
            "Pew-Pew (L{} | Score: {} | Lives: {})",
            self.level, self.score, self.lives
        )
    }

    fn render_mode(&self) -> PluginRenderMode {
        match self.state {
            GameState::Playing => PluginRenderMode::KittyGraphics,
            GameState::Menu
            | GameState::Paused
            | GameState::GameOver
            | GameState::LevelComplete => PluginRenderMode::Text,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer, _ctx: &PluginContext) {
        let lines: Vec<Line> = match self.state {
            GameState::Menu => vec![
                Line::styled(
                    "PEW-PEW",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Line::from(""),
                Line::from("Controls:"),
                Line::from("  A/D or ←/→ : Move"),
                Line::from("  Space      : Shoot"),
                Line::from("  P          : Pause"),
                Line::from("  R          : Restart"),
                Line::from(""),
                Line::styled(
                    "Press SPACE to start",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ],
            GameState::Paused => vec![
                Line::from(""),
                Line::styled(
                    "PAUSED",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Line::from(""),
                Line::from("Press P to resume"),
            ],
            GameState::GameOver => vec![
                Line::from(""),
                Line::styled(
                    "GAME OVER",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Line::from(""),
                Line::from(format!("Final Score: {}", self.score)),
                Line::from(format!("Level Reached: {}", self.level)),
                Line::from(format!("High Score: {}", self.high_score)),
                Line::from(""),
                Line::styled(
                    "Press R to restart",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ],
            GameState::LevelComplete => vec![
                Line::from(""),
                Line::styled(
                    "LEVEL COMPLETE!",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Line::from(""),
                Line::from(format!("Level {} cleared!", self.level)),
                Line::from(format!("Score: {}", self.score)),
                Line::from(""),
                Line::styled(
                    "Press SPACE for next level",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ],
            GameState::Playing => Vec::new(),
        };

        Paragraph::new(lines)
            .alignment(ratatui::layout::Alignment::Center)
            .render(area, buf);
    }

    fn render_frame(&mut self, _width: u32, _height: u32) -> Option<PluginFrame> {
        if self.state != GameState::Playing {
            return None;
        }

        if self.frame_ready {
            self.render_game_frame();
            self.frame_ready = false;

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
        let Event::Key(KeyEvent {
            code,
            modifiers,
            kind,
            ..
        }) = event
        else {
            return PluginEventResult::Ignored;
        };

        let no_modifiers = *modifiers == KeyModifiers::NONE;
        let is_press = *kind == KeyEventKind::Press || *kind == KeyEventKind::Repeat;
        let is_release = *kind == KeyEventKind::Release;

        // Movement keys (press + release)
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
                _ => {}
            }
        }

        // Only handle press events for actions
        if !is_press {
            return PluginEventResult::Ignored;
        }

        match (self.state, code, no_modifiers) {
            (GameState::Menu, KeyCode::Char(' '), true) => {
                self.reset_game();
                PluginEventResult::Consumed
            }

            (GameState::Playing, KeyCode::Char(' '), true) => {
                // Fire bullet from center of ship
                let bullet_x = self.player_x + PLAYER_WIDTH / 2.0 - 2.0;
                let bullet_y = PLAYER_Y;
                self.projectiles.fire_player(bullet_x, bullet_y);
                PluginEventResult::Consumed
            }
            (GameState::Playing, KeyCode::Char('p') | KeyCode::Char('P'), true) => {
                self.state = GameState::Paused;
                PluginEventResult::Consumed
            }
            (GameState::Playing, KeyCode::Char('r') | KeyCode::Char('R'), true) => {
                self.reset_game();
                PluginEventResult::Consumed
            }

            (GameState::Paused, KeyCode::Char('p') | KeyCode::Char('P'), true) => {
                self.state = GameState::Playing;
                PluginEventResult::Consumed
            }

            (GameState::GameOver, KeyCode::Char('r') | KeyCode::Char('R'), true) => {
                self.reset_game();
                PluginEventResult::Consumed
            }

            (GameState::LevelComplete, KeyCode::Char(' '), true) => {
                self.level = self.level.saturating_add(1);
                self.load_level();
                self.state = GameState::Playing;
                PluginEventResult::Consumed
            }

            _ => PluginEventResult::Ignored,
        }
    }

    fn tick(&mut self) -> bool {
        // Fixed time step of ~60fps (16.67ms)
        const DT: f32 = 1.0 / 60.0;
        self.update(DT);
        self.frame_ready
    }

    fn on_activate(&mut self) {
        self.keys = KeyState::default();
    }

    fn on_deactivate(&mut self) {
        if self.state == GameState::Playing {
            self.state = GameState::Paused;
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Default for PewPewPlugin {
    fn default() -> Self {
        Self::new()
    }
}
