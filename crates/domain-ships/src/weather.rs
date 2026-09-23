//! Clima do mar (MF-059): vento global que gira devagar e células de
//! tempestade que nascem em águas de risco. Puro e determinístico por
//! semente — o servidor só injeta `dt`, a área e onde é proibido chover.

use std::f32::consts::TAU;
use std::f64::consts::TAU as TAU64;

use crate::sailing::Wind;

/// Giro médio do vento: uma volta completa a cada 20 min.
const MEAN_TURN_RATE: f32 = TAU / (20.0 * 60.0);
/// Oscilação do giro (fração do giro médio) e seu período.
const TURN_WOBBLE: f32 = 0.8;
const TURN_WOBBLE_PERIOD: f32 = 300.0;
/// Passeio aleatório da velocidade angular (rad/s por s) e seu teto.
const TURN_WALK_ACCEL: f32 = 0.0016;
const TURN_WALK_MAX: f32 = 0.004;

const STRENGTH_MIN: f32 = 0.4;
const STRENGTH_MAX: f32 = 1.0;

const MAX_STORMS: usize = 2;
const STORM_RADIUS: (f32, f32) = (250.0, 400.0);
const STORM_LIFETIME: (f32, f32) = (180.0, 360.0);
/// Intervalo entre tentativas de nascer uma tempestade.
const STORM_INTERVAL: (f32, f32) = (60.0, 180.0);
/// Primeira tempestade do servidor.
const FIRST_STORM: (f32, f32) = (20.0, 60.0);
/// Deriva com o vento (m/s na força máxima).
const STORM_DRIFT: f32 = 6.0;
const SPAWN_ATTEMPTS: usize = 24;
/// Pontos do perímetro checados contra águas proibidas.
const RIM_SAMPLES: usize = 12;

/// Retângulo do mar onde tempestades podem nascer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeatherBounds {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Storm {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub age: f32,
    pub lifetime: f32,
}

impl Storm {
    /// Segundos de fade in/out.
    pub const FADE_SECS: f32 = 20.0;

    /// 0..1: sobe no nascimento, desce no fim da vida.
    pub fn intensity(&self) -> f32 {
        (self.age / Self::FADE_SECS)
            .min((self.lifetime - self.age) / Self::FADE_SECS)
            .clamp(0.0, 1.0)
    }

    /// Influência 0..1 num ponto: plena até 80% do raio, some na borda.
    pub fn influence_at(&self, x: f32, y: f32) -> f32 {
        let d = ((x - self.x).powi(2) + (y - self.y).powi(2)).sqrt();
        let edge = (1.0 - (d - self.radius * 0.8) / (self.radius * 0.2)).clamp(0.0, 1.0);
        edge * self.intensity()
    }

    fn circle_allowed(&self, allowed: &impl Fn(f32, f32) -> bool) -> bool {
        circle_allowed(self.x, self.y, self.radius, allowed)
    }
}

fn circle_allowed(x: f32, y: f32, radius: f32, allowed: &impl Fn(f32, f32) -> bool) -> bool {
    allowed(x, y)
        && (0..RIM_SAMPLES).all(|i| {
            let a = TAU * i as f32 / RIM_SAMPLES as f32;
            allowed(x + a.cos() * radius, y + a.sin() * radius)
        })
}

#[derive(Debug, Clone, PartialEq)]
pub struct Weather {
    rng: u64,
    // f64: em f32 o `time += dt` para de andar após ~6 dias de uptime.
    time: f64,
    wind_dir: f32,
    turn_walk: f32,
    strength_phase: (f32, f32),
    storms: Vec<Storm>,
    next_storm_in: f32,
    next_storm_id: u32,
}

impl Weather {
    pub fn new(seed: u64) -> Self {
        let mut weather = Self {
            rng: seed ^ 0x9E37_79B9_7F4A_7C15,
            time: 0.0,
            wind_dir: 0.0,
            turn_walk: 0.0,
            strength_phase: (0.0, 0.0),
            storms: Vec::new(),
            next_storm_in: 0.0,
            next_storm_id: 1,
        };
        weather.wind_dir = weather.uniform(0.0, TAU);
        weather.strength_phase = (weather.uniform(0.0, TAU), weather.uniform(0.0, TAU));
        weather.next_storm_in = weather.uniform(FIRST_STORM.0, FIRST_STORM.1);
        weather
    }

    /// splitmix64 → [0, 1).
    fn next_f32(&mut self) -> f32 {
        self.rng = self.rng.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 40) as f32 / (1u64 << 24) as f32
    }

    fn uniform(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next_f32()
    }

    /// Vento global (fora das tempestades).
    pub fn base_wind(&self) -> Wind {
        let t = self.time;
        let (p1, p2) = self.strength_phase;
        let wave = |period: f64, phase: f32| (TAU64 * t / period + f64::from(phase)).sin() as f32;
        let strength = 0.7 + 0.2 * wave(420.0, p1) + 0.1 * wave(97.0, p2);
        Wind {
            direction: self.wind_dir,
            strength: strength.clamp(STRENGTH_MIN, STRENGTH_MAX),
        }
    }

    pub fn storms(&self) -> &[Storm] {
        &self.storms
    }

    /// Maior influência de tempestade num ponto (0 = céu limpo).
    pub fn storm_influence(&self, x: f32, y: f32) -> f32 {
        self.storms
            .iter()
            .map(|storm| storm.influence_at(x, y))
            .fold(0.0, f32::max)
    }

    /// Vento local: dentro da tempestade sopra a toda força e com rajadas
    /// que torcem a direção — suaves no tempo, nunca saltos.
    pub fn wind_at(&self, x: f32, y: f32) -> Wind {
        let base = self.base_wind();
        let Some((storm, k)) = self
            .storms
            .iter()
            .map(|storm| (storm, storm.influence_at(x, y)))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .filter(|(_, k)| *k > 0.0)
        else {
            return base;
        };
        let id = f64::from(storm.id);
        let t = self.time;
        let gust = (0.45 * (t * 0.9 + id * 1.3).sin() + 0.25 * (t * 2.3 + id * 0.7).sin()) as f32;
        Wind {
            direction: (base.direction + gust * k).rem_euclid(TAU),
            strength: base.strength + (1.0 - base.strength) * k,
        }
    }

    /// Avança o clima. `allowed(x, y)` diz onde uma tempestade pode estar
    /// (o servidor proíbe águas protegidas de porto).
    pub fn step(&mut self, dt: f32, bounds: WeatherBounds, allowed: impl Fn(f32, f32) -> bool) {
        if dt <= 0.0 {
            return;
        }
        self.time += f64::from(dt);

        let noise = self.uniform(-1.0, 1.0);
        self.turn_walk =
            (self.turn_walk + noise * TURN_WALK_ACCEL * dt).clamp(-TURN_WALK_MAX, TURN_WALK_MAX);
        let wobble =
            1.0 + TURN_WOBBLE * (TAU64 * self.time / f64::from(TURN_WOBBLE_PERIOD)).sin() as f32;
        self.wind_dir =
            (self.wind_dir + (MEAN_TURN_RATE * wobble + self.turn_walk) * dt).rem_euclid(TAU);

        let wind = self.base_wind();
        let drift = STORM_DRIFT * wind.strength;
        for storm in &mut self.storms {
            storm.age += dt;
            storm.x += wind.direction.cos() * drift * dt;
            storm.y += wind.direction.sin() * drift * dt;
            // Derivou para águas proibidas: começa a se desfazer já.
            let fading = storm.lifetime - storm.age <= Storm::FADE_SECS;
            if !fading && !storm.circle_allowed(&allowed) {
                storm.lifetime = storm.age + Storm::FADE_SECS;
            }
        }
        self.storms.retain(|storm| storm.age < storm.lifetime);

        self.next_storm_in -= dt;
        if self.next_storm_in <= 0.0 {
            self.next_storm_in = self.uniform(STORM_INTERVAL.0, STORM_INTERVAL.1);
            if self.storms.len() < MAX_STORMS {
                self.try_spawn_storm(bounds, &allowed);
            }
        }
    }

    fn try_spawn_storm(&mut self, bounds: WeatherBounds, allowed: &impl Fn(f32, f32) -> bool) {
        for _ in 0..SPAWN_ATTEMPTS {
            let x = self.uniform(bounds.min_x, bounds.max_x);
            let y = self.uniform(bounds.min_y, bounds.max_y);
            let radius = self.uniform(STORM_RADIUS.0, STORM_RADIUS.1);
            if circle_allowed(x, y, radius, allowed) {
                let lifetime = self.uniform(STORM_LIFETIME.0, STORM_LIFETIME.1);
                self.storms.push(Storm {
                    id: self.next_storm_id,
                    x,
                    y,
                    radius,
                    age: 0.0,
                    lifetime,
                });
                self.next_storm_id += 1;
                return;
            }
        }
    }

    /// Teste/dev: fixa a direção atual do vento (o giro lento continua).
    pub fn with_wind_direction(mut self, direction: f32) -> Self {
        self.wind_dir = direction.rem_euclid(TAU);
        self
    }

    /// Dev/teste: força uma tempestade (ignora a área, não a regra de águas).
    pub fn spawn_storm_at(&mut self, x: f32, y: f32, radius: f32, lifetime: f32) {
        self.storms.push(Storm {
            id: self.next_storm_id,
            x,
            y,
            radius,
            age: 0.0,
            lifetime,
        });
        self.next_storm_id += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOUNDS: WeatherBounds = WeatherBounds {
        min_x: -1500.0,
        min_y: -1500.0,
        max_x: 1500.0,
        max_y: 1500.0,
    };
    const DT: f32 = 1.0 / 30.0;

    /// Porto protegido em (0, 0) com raio 600.
    fn open_sea(x: f32, y: f32) -> bool {
        x * x + y * y > 600.0 * 600.0
    }

    fn run(seed: u64, secs: f32) -> Weather {
        let mut weather = Weather::new(seed);
        for _ in 0..(secs / DT) as usize {
            weather.step(DT, BOUNDS, open_sea);
        }
        weather
    }

    #[test]
    fn same_seed_same_weather() {
        let a = run(42, 600.0);
        let b = run(42, 600.0);
        assert_eq!(a, b);
        assert_ne!(a.base_wind(), run(7, 600.0).base_wind());
    }

    #[test]
    fn wind_turns_smoothly_and_strength_stays_in_range() {
        let mut weather = Weather::new(3);
        let mut prev = weather.base_wind();
        let mut turned = 0.0;
        for _ in 0..(30 * 60 * 25) {
            weather.step(DT, BOUNDS, open_sea);
            let wind = weather.base_wind();
            let delta = (wind.direction - prev.direction + std::f32::consts::PI).rem_euclid(TAU)
                - std::f32::consts::PI;
            assert!(delta.abs() < 0.002, "vento nunca salta: {delta}");
            assert!((STRENGTH_MIN..=STRENGTH_MAX).contains(&wind.strength));
            turned += delta;
            prev = wind;
        }
        // 25 min: mais ou menos uma volta (giro médio de 20 min).
        assert!(turned.abs() > TAU * 0.7, "turned={turned}");
    }

    #[test]
    fn storms_never_spawn_in_protected_waters() {
        let mut weather = Weather::new(11);
        let mut spawned = 0;
        let mut seen = std::collections::HashSet::new();
        for _ in 0..(30 * 60 * 40) {
            weather.step(DT, BOUNDS, open_sea);
            for storm in weather.storms() {
                if seen.insert(storm.id) {
                    spawned += 1;
                    assert!(storm.circle_allowed(&open_sea), "{storm:?}");
                    assert!((STORM_RADIUS.0..=STORM_RADIUS.1).contains(&storm.radius));
                }
            }
            assert!(weather.storms().len() <= MAX_STORMS);
        }
        assert!(spawned >= 5, "spawned={spawned}");
    }

    #[test]
    fn storm_drifting_into_protected_waters_fades_out() {
        let mut weather = Weather::new(1);
        weather.spawn_storm_at(650.0, 0.0, 300.0, 300.0);
        weather.step(DT, BOUNDS, open_sea);
        let storm = weather.storms()[0];
        assert!(storm.lifetime - storm.age <= Storm::FADE_SECS + 1e-3);
    }

    #[test]
    fn storm_wind_is_full_strength_and_fades_in() {
        let mut weather = Weather::new(5);
        weather.spawn_storm_at(1000.0, 1000.0, 300.0, 200.0);
        assert_eq!(
            weather.storm_influence(1000.0, 1000.0),
            0.0,
            "nasce invisível"
        );
        for _ in 0..(30 * 25) {
            weather.step(DT, BOUNDS, |_, _| true);
        }
        let storm = weather.storms()[0];
        assert!((weather.wind_at(storm.x, storm.y).strength - 1.0).abs() < 1e-5);
        assert_eq!(weather.storm_influence(storm.x + 500.0, storm.y), 0.0);
        assert_eq!(
            weather.wind_at(storm.x + 500.0, storm.y),
            weather.base_wind()
        );
    }
}
