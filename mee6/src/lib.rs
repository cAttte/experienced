#![deny(clippy::all, clippy::pedantic, clippy::nursery)]
// We allow cast precision loss because we will never be messing with integers bigger then 52 bits realistically
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
//! A library to calculate mee6 levels.
//! This can be calculated using the `LevelInfo` struct.

/// `LevelInfo` stores all of the data calculated when using `LevelInfo::new`(), so it can be cheaply
/// gotten with getters.
#[derive(Copy, Clone, PartialEq, PartialOrd, Debug)]
pub struct LevelInfo {
    xp: u64,
    level: u64,
    percentage: f64,
}

impl LevelInfo {
    /// Create a new `LevelInfo` struct. This operation calculates the current percentage and level
    /// immediately, rather then when the getter is called.
    #[must_use]
    pub fn new(xp: u64) -> Self {
        let level = {
            let mut testxp = 0;
            let mut level = 0;
            while xp >= testxp {
                level += 1;
                testxp = xp_needed_for_level(level);
            }
            level - 1
        };
        let last_level_xp_requirement = xp_needed_for_level(level);
        let next_level_xp_requirement = xp_needed_for_level(level + 1);
        Self {
            xp,
            level,
            percentage: ((xp as f64 - last_level_xp_requirement as f64)
                / (next_level_xp_requirement as f64 - last_level_xp_requirement as f64)),
        }
    }

    /// Get the xp that was input into this `LevelInfo`.
    #[must_use]
    #[inline]
    pub const fn xp(&self) -> u64 {
        self.xp
    }

    /// Get the level that this `LevelInfo` represents.
    #[must_use]
    #[inline]
    pub const fn level(&self) -> u64 {
        self.level
    }

    /// Get the percentage of the way this `LevelInfo` is to gaining a level, from the last level.
    #[must_use]
    #[inline]
    pub const fn percentage(&self) -> f64 {
        self.percentage
    }
    // mul_add is not no-std
}

#[allow(clippy::suboptimal_flops)]
#[inline]
#[must_use]
pub fn xp_needed_for_level(level: u64) -> u64 {
    // "secret level" feature (artificial xp wall)
    if level > 30 {
        return xp_needed_for_level(30) * (level - 29);
    }

    let base_xp = 6_f64 + (level as f64).powf(3.1155);
    nice_round(base_xp) as u64
}

#[inline]
#[must_use]
fn nice_round(num: f64) -> f64 {
    let multiple = (10_f64).powf((num.log10() / 2.0).floor());
    (num / multiple).round() * multiple
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn level() {
        let inf = LevelInfo::new(3255);
        assert_eq!(inf.level(), 8);
    }
    #[test]
    fn xp() {
        let inf = LevelInfo::new(3255);
        assert_eq!(inf.xp(), 3255);
    }
    #[test]
    fn percentage() {
        let inf = LevelInfo::new(3255);
        assert!((inf.percentage() - 0.43).abs() > f64::EPSILON);
    }
}
