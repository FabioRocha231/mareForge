//! Vento e pano (MF-059): regras puras de ponto de vela e de velas rasgadas.
//! Navegar é habilidade — o vento decide quanto o pano rende, e ninguém
//! aponta a proa contra o vento e sai andando.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};

use serde::{Deserialize, Serialize};

/// Vento num ponto do mar. `direction` é para ONDE o vento sopra (radianos,
/// mesma convenção do heading: 0 = +X, anti-horário); `strength` em [0, 1].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Wind {
    pub direction: f32,
    pub strength: f32,
}

/// Curva polar (θ fora do vento → fator de velocidade). θ = 0 é vento de
/// popa; θ = π é proa ao vento. Entre os nós a curva é suave (smoothstep),
/// monótona em cada trecho; de 5π/6 em diante o navio está "em irons".
const POLAR: [(f32, f32); 6] = [
    (0.0, 0.75),
    (FRAC_PI_4, 0.95),
    (FRAC_PI_2, 1.05),
    (2.0 * PI / 3.0, 0.85),
    (3.0 * PI / 4.0, 0.55),
    (5.0 * PI / 6.0, 0.15),
];

/// Fator do pano no ângulo `theta` (radianos, clampeado para [0, π]).
pub fn polar_factor(theta: f32) -> f32 {
    let theta = theta.clamp(0.0, PI);
    for pair in POLAR.windows(2) {
        let ((a, fa), (b, fb)) = (pair[0], pair[1]);
        if theta <= b {
            let t = (theta - a) / (b - a);
            let s = t * t * (3.0 - 2.0 * t);
            return fa + (fb - fa) * s;
        }
    }
    POLAR[POLAR.len() - 1].1
}

/// Ângulo entre a proa e a direção do vento, em [0, π].
pub fn angle_off_wind(heading: f32, wind_direction: f32) -> f32 {
    let d = (heading - wind_direction).rem_euclid(TAU);
    if d > PI {
        TAU - d
    } else {
        d
    }
}

/// Multiplicador de velocidade do vento para uma proa:
/// `polar(θ) * (0.75 + 0.35 * strength)`.
pub fn wind_speed_factor(heading: f32, wind: Wind) -> f32 {
    polar_factor(angle_off_wind(heading, wind.direction))
        * (0.75 + 0.35 * wind.strength.clamp(0.0, 1.0))
}

/// Ponto de vela para a UI (o servidor não usa: a regra é a curva).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointOfSail {
    /// Vento de popa/alheta.
    Running,
    /// Vento de través — o melhor rendimento.
    BeamReach,
    /// Bolina: dá para subir o vento, devagar.
    CloseHauled,
    /// Proa ao vento: o pano panejando.
    InIrons,
}

pub fn point_of_sail(theta: f32) -> PointOfSail {
    if theta < PI / 3.0 {
        PointOfSail::Running
    } else if theta < 7.0 * PI / 12.0 {
        PointOfSail::BeamReach
    } else if theta < 19.0 * PI / 24.0 {
        PointOfSail::CloseHauled
    } else {
        PointOfSail::InIrons
    }
}

/// Velas inteiras.
pub const SAIL_HP_MAX: f32 = 100.0;
/// Remendo em alto-mar: +1 ponto a cada 3 s.
pub const SAIL_REPAIR_PER_SEC: f32 = 1.0 / 3.0;
/// Tempestade rasga o pano (por segundo, na intensidade máxima).
pub const STORM_SAIL_DAMAGE_PER_SEC: f32 = 2.0;

/// Pano rasgado rende menos: `0.35 + 0.65 * sail_hp / 100`.
pub fn sail_speed_multiplier(sail_hp: f32) -> f32 {
    0.35 + 0.65 * (sail_hp / SAIL_HP_MAX).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::motion::{step_motion, MotionInput, MotionTuning, ShipMotion};
    use crate::stats::ShipStats;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn polar_hits_the_knots() {
        for (theta, factor) in POLAR {
            assert!(close(polar_factor(theta), factor), "θ={theta}");
        }
    }

    #[test]
    fn polar_rises_to_beam_reach_then_falls() {
        let samples = 200;
        let mut prev = polar_factor(0.0);
        for i in 1..=samples {
            let f = polar_factor(FRAC_PI_2 * i as f32 / samples as f32);
            assert!(f >= prev - 1e-6, "sobe até o través");
            prev = f;
        }
        for i in 1..=samples {
            let theta = FRAC_PI_2 + FRAC_PI_2 * i as f32 / samples as f32;
            let f = polar_factor(theta);
            assert!(f <= prev + 1e-6, "cai do través até a proa ao vento");
            prev = f;
        }
    }

    #[test]
    fn in_irons_is_clamped() {
        for theta in [5.0 * PI / 6.0, 0.9 * PI, PI] {
            assert!(close(polar_factor(theta), 0.15));
        }
        assert!(close(polar_factor(10.0), 0.15), "θ acima de π clampa");
    }

    #[test]
    fn angle_off_wind_is_symmetric_and_bounded() {
        assert!(close(angle_off_wind(0.0, 0.0), 0.0));
        assert!(close(angle_off_wind(PI, 0.0), PI));
        assert!(close(angle_off_wind(0.1, TAU - 0.1), 0.2));
        assert!(close(angle_off_wind(-FRAC_PI_2, 0.0), FRAC_PI_2));
    }

    fn merchant() -> ShipStats {
        ShipStats {
            speed: 30.0,
            turn_rate: 1.0,
            max_hp: 100,
            cargo_capacity: 100,
            weapon_damage: 20,
            weapon_range: 210.0,
        }
    }

    #[test]
    fn tuning_sanity_beam_reach_keeps_cruise_and_upwind_crawls() {
        let wind = Wind {
            direction: 0.0,
            strength: 0.7,
        };
        let beam = 30.0 * wind_speed_factor(FRAC_PI_2, wind);
        let upwind = 30.0 * wind_speed_factor(PI, wind);
        assert!((28.0..=34.0).contains(&beam), "beam={beam}");
        assert!((3.5..=6.0).contains(&upwind), "upwind={upwind}");
    }

    fn steer(motion: &ShipMotion, target: f32) -> f32 {
        let err = (target - motion.heading + PI).rem_euclid(TAU) - PI;
        (err * 3.0).clamp(-1.0, 1.0)
    }

    /// Vento soprando para +X: "subir o vento" é ir para -X.
    fn upwind_progress(tack: bool) -> f32 {
        let wind = Wind {
            direction: 0.0,
            strength: 0.7,
        };
        let tuning = MotionTuning::default();
        let mut motion = ShipMotion {
            heading: PI,
            ..ShipMotion::default()
        };
        let dt = 1.0 / 30.0;
        for tick in 0..(30 * 180) {
            let target = if tack {
                // Bordos de 30 s alternando a 2π/3 do vento.
                let off = 2.0 * PI / 3.0;
                if (tick / (30 * 30)) % 2 == 0 {
                    off
                } else {
                    -off
                }
            } else {
                PI
            };
            let input = MotionInput {
                throttle: 1.0,
                turn: steer(&motion, target),
            };
            step_motion(&mut motion, &merchant(), input, wind, &tuning, dt);
        }
        -motion.x
    }

    #[test]
    fn tacking_beats_pointing_straight_into_the_wind() {
        let straight = upwind_progress(false);
        let tacking = upwind_progress(true);
        assert!(straight > 0.0, "até em irons o navio arrasta um pouco");
        assert!(
            tacking > straight * 1.8,
            "bordejar sobe o vento: tacking={tacking} straight={straight}"
        );
    }

    #[test]
    fn sail_multiplier_spans_035_to_1() {
        assert!(close(sail_speed_multiplier(100.0), 1.0));
        assert!(close(sail_speed_multiplier(0.0), 0.35));
        assert!(close(sail_speed_multiplier(50.0), 0.675));
        assert!(close(sail_speed_multiplier(-5.0), 0.35));
        assert!(close(sail_speed_multiplier(250.0), 1.0));
    }

    #[test]
    fn point_of_sail_labels_follow_the_polar() {
        assert_eq!(point_of_sail(0.2), PointOfSail::Running);
        assert_eq!(point_of_sail(FRAC_PI_2), PointOfSail::BeamReach);
        assert_eq!(point_of_sail(2.0 * PI / 3.0), PointOfSail::CloseHauled);
        assert_eq!(point_of_sail(PI), PointOfSail::InIrons);
    }
}
