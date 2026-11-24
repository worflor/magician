//! The fair-play limits a lint run judges against, as one value the run is handed (`Input::fair_play`).
//!
//! The numbers themselves have one home each, the constants block in `mesh/rules.rs`; `DEFAULT` is built from them,
//! so a run with no policy file judges exactly as before. A policy file (read by `grade::policy`, selected with
//! `--policy`) may move any of them within the range below, and the findings cache keys on the values in force, so a
//! cached verdict is never served under different limits.

use crate::mesh::rules as r;

/// Every fair-play limit the rules read. The field names are the `[fair_play]` keys of a policy file.
#[derive(Debug, Clone, PartialEq)]
pub struct FairPlay {
    pub silhouette_min: f64,
    pub hitbox_warn_units: f64,
    pub hitbox_warn_share: f64,
    pub hitbox_taste_units: f64,
    pub hitbox_egregious_units: f64,
    pub hitbox_egregious_share: f64,
    pub hitbox_noise_area: f64,
    pub particle_energy_ratio: f64,
    pub particle_radius_ratio: f64,
    pub light_lumens_ratio: f64,
    pub light_output_ratio: f64,
    pub light_range_ratio: f64,
    pub sound_volume_slack_db: f64,
    pub sound_duration_ratio: f64,
    pub sound_peak_slack_db: f64,
}

/// Magician's own line: the constants in `mesh/rules.rs`.
pub const DEFAULT: FairPlay = FairPlay {
    silhouette_min: r::SILHOUETTE_MIN,
    hitbox_warn_units: r::HITBOX_WARN_UNITS,
    hitbox_warn_share: r::HITBOX_WARN_SHARE,
    hitbox_taste_units: r::HITBOX_TASTE_UNITS,
    hitbox_egregious_units: r::HITBOX_EGREGIOUS_UNITS,
    hitbox_egregious_share: r::HITBOX_EGREGIOUS_SHARE,
    hitbox_noise_area: r::HITBOX_NOISE_AREA,
    particle_energy_ratio: r::PARTICLE_ENERGY_RATIO,
    particle_radius_ratio: r::PARTICLE_RADIUS_RATIO,
    light_lumens_ratio: r::LIGHT_LUMENS_RATIO,
    light_output_ratio: r::LIGHT_OUTPUT_RATIO,
    light_range_ratio: r::LIGHT_RANGE_RATIO,
    sound_volume_slack_db: r::SOUND_VOLUME_SLACK_DB,
    sound_duration_ratio: r::SOUND_DURATION_RATIO,
    sound_peak_slack_db: r::SOUND_PEAK_SLACK_DB,
};

impl Default for FairPlay {
    fn default() -> Self {
        DEFAULT
    }
}

/// One tunable limit: its key, the rules it moves, the values a policy may give it, and what it means.
pub struct Key {
    pub name: &'static str,
    pub rules: &'static str,
    pub min: f64,
    pub max: f64,
    pub what: &'static str,
}

/// The `[fair_play]` keys, in the order `magician policy` prints them.
pub const KEYS: &[Key] = &[
    Key { name: "silhouette_min", rules: "M124", min: 0.5, max: 1.0, what: "our in-game silhouette may not fall below this share of Valve's in any view" },
    Key { name: "hitbox_warn_units", rules: "M130", min: 0.0, max: 100.0, what: "distance beyond every hitbox capsule the wide-envelope share is measured past" },
    Key { name: "hitbox_warn_share", rules: "M130", min: 0.0, max: 1.0, what: "share of the surface past hitbox_warn_units that warns" },
    Key { name: "hitbox_taste_units", rules: "M140", min: 0.0, max: 100.0, what: "visible surface past this distance needs a written reason (the taste tier)" },
    Key { name: "hitbox_egregious_units", rules: "M129", min: 0.0, max: 200.0, what: "a visible point past this distance is 'a bit much': an error" },
    Key { name: "hitbox_egregious_share", rules: "M129", min: 0.0, max: 1.0, what: "more than this share of the visible surface past hitbox_taste_units is an error" },
    Key { name: "hitbox_noise_area", rules: "M129 M140", min: 0.0, max: 100.0, what: "visible area (u2) past hitbox_taste_units below which nothing is said" },
    Key { name: "particle_energy_ratio", rules: "M081", min: 1.0, max: 100.0, what: "a replaced particle may be this many times brighter than Valve's (colour x alpha x overbright)" },
    Key { name: "particle_radius_ratio", rules: "M081", min: 1.0, max: 100.0, what: "a replaced particle's largest sprite may be this many times Valve's radius" },
    Key { name: "light_lumens_ratio", rules: "M082", min: 1.0, max: 100.0, what: "a particle light may be this many times Valve's lumens" },
    Key { name: "light_output_ratio", rules: "M082", min: 1.0, max: 100.0, what: "a particle light's colour may give this many times Valve's luminous output" },
    Key { name: "light_range_ratio", rules: "M082", min: 1.0, max: 100.0, what: "a particle light may reach this many times Valve's range" },
    Key { name: "sound_volume_slack_db", rules: "M063", min: 0.0, max: 24.0, what: "a replaced sound event may play this many dB quieter than Valve's" },
    Key { name: "sound_duration_ratio", rules: "M063", min: 0.0, max: 1.0, what: "a replaced sound may be as short as this share of Valve's length" },
    Key { name: "sound_peak_slack_db", rules: "M063", min: 0.0, max: 24.0, what: "a replaced sound file's envelope peak may sit this many dB under Valve's" },
];

impl FairPlay {
    pub fn get(&self, key: &str) -> Option<f64> {
        Some(match key {
            "silhouette_min" => self.silhouette_min,
            "hitbox_warn_units" => self.hitbox_warn_units,
            "hitbox_warn_share" => self.hitbox_warn_share,
            "hitbox_taste_units" => self.hitbox_taste_units,
            "hitbox_egregious_units" => self.hitbox_egregious_units,
            "hitbox_egregious_share" => self.hitbox_egregious_share,
            "hitbox_noise_area" => self.hitbox_noise_area,
            "particle_energy_ratio" => self.particle_energy_ratio,
            "particle_radius_ratio" => self.particle_radius_ratio,
            "light_lumens_ratio" => self.light_lumens_ratio,
            "light_output_ratio" => self.light_output_ratio,
            "light_range_ratio" => self.light_range_ratio,
            "sound_volume_slack_db" => self.sound_volume_slack_db,
            "sound_duration_ratio" => self.sound_duration_ratio,
            "sound_peak_slack_db" => self.sound_peak_slack_db,
            _ => return None,
        })
    }

    /// Set one key, checked against its range. The error names the key and the range.
    pub fn set(&mut self, key: &str, v: f64) -> Result<(), String> {
        let k = KEYS.iter().find(|k| k.name == key).ok_or_else(|| format!("unknown key fair_play.{key}"))?;
        if !v.is_finite() || v < k.min || v > k.max {
            return Err(format!("fair_play.{key} = {v} is out of range ({} to {})", k.min, k.max));
        }
        let slot = match key {
            "silhouette_min" => &mut self.silhouette_min,
            "hitbox_warn_units" => &mut self.hitbox_warn_units,
            "hitbox_warn_share" => &mut self.hitbox_warn_share,
            "hitbox_taste_units" => &mut self.hitbox_taste_units,
            "hitbox_egregious_units" => &mut self.hitbox_egregious_units,
            "hitbox_egregious_share" => &mut self.hitbox_egregious_share,
            "hitbox_noise_area" => &mut self.hitbox_noise_area,
            "particle_energy_ratio" => &mut self.particle_energy_ratio,
            "particle_radius_ratio" => &mut self.particle_radius_ratio,
            "light_lumens_ratio" => &mut self.light_lumens_ratio,
            "light_output_ratio" => &mut self.light_output_ratio,
            "light_range_ratio" => &mut self.light_range_ratio,
            "sound_volume_slack_db" => &mut self.sound_volume_slack_db,
            "sound_duration_ratio" => &mut self.sound_duration_ratio,
            _ => &mut self.sound_peak_slack_db,
        };
        *slot = v;
        Ok(())
    }

    /// Limits that only make sense together: the envelope tiers must stay in order (warn, taste, egregious).
    pub fn check(&self) -> Result<(), String> {
        if self.hitbox_warn_units > self.hitbox_taste_units {
            return Err(format!("fair_play.hitbox_warn_units ({}) must not exceed fair_play.hitbox_taste_units ({})", self.hitbox_warn_units, self.hitbox_taste_units));
        }
        if self.hitbox_taste_units >= self.hitbox_egregious_units {
            return Err(format!("fair_play.hitbox_taste_units ({}) must be below fair_play.hitbox_egregious_units ({})", self.hitbox_taste_units, self.hitbox_egregious_units));
        }
        Ok(())
    }

    /// Text that names these values exactly, for the findings cache key and the policy fingerprint.
    pub fn fingerprint(&self) -> String {
        KEYS.iter().map(|k| format!("{}={}", k.name, self.get(k.name).unwrap_or(f64::NAN))).collect::<Vec<_>>().join(";")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_reads_and_writes_its_own_field() {
        for (i, k) in KEYS.iter().enumerate() {
            let mut p = DEFAULT;
            let v = if k.max > k.min + 1.0 { k.min + 1.0 } else { (k.min + k.max) / 2.0 };
            assert_ne!(Some(v), DEFAULT.get(k.name), "{}: pick a value that differs from the default", k.name);
            p.set(k.name, v).unwrap();
            assert_eq!(p.get(k.name), Some(v), "{}", k.name);
            for other in KEYS.iter().filter(|o| o.name != k.name) {
                assert_eq!(p.get(other.name), DEFAULT.get(other.name), "setting {} moved {}", k.name, other.name);
            }
            assert_ne!(p.fingerprint(), DEFAULT.fingerprint(), "key {i} must change the fingerprint");
        }
    }

    #[test]
    fn defaults_are_inside_their_ranges_and_in_order() {
        for k in KEYS {
            let v = DEFAULT.get(k.name).unwrap();
            assert!(v >= k.min && v <= k.max, "{} default {v} outside {}..{}", k.name, k.min, k.max);
        }
        DEFAULT.check().unwrap();
    }

    #[test]
    fn out_of_range_and_unknown_keys_are_named() {
        let mut p = DEFAULT;
        assert!(p.set("silhouette_min", 1.2).unwrap_err().contains("fair_play.silhouette_min"));
        assert!(p.set("hitbox_egregious_share", -0.1).is_err());
        assert!(p.set("hitbox_units", 1.0).unwrap_err().contains("unknown key fair_play.hitbox_units"));
        p.set("hitbox_taste_units", 20.0).unwrap();
        assert!(p.check().unwrap_err().contains("hitbox_egregious_units"), "the taste line may not pass the egregious line");
    }
}
