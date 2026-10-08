//! N64 combiner -> GX TEV translation.
//!
//! `combine_auto()` compiles the N64 combiner expression (plus the
//! 1-cycle/2-cycle othermode bit) into GX TEV stages; `combine_tev()`
//! and `combine_manual()` are the explicit/manual fallbacks. Expressions
//! mentioning TEXEL1/TEXEL1_ALPHA fall back to `G_SETCOMBINE_NOTEV`
//! (`replace_combine_to_tev`), and pre-converted combiners arrive as
//! `G_SETCOMBINE_TEV`.

/// Which translation path produced the TEV state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CombinePath {
    /// `combine_auto`: dynamic N64 -> TEV compilation.
    #[default]
    Auto,
    /// `combine_tev`: explicit TEV path.
    Tev,
    /// `combine_manual`: fallback for odd expressions.
    Manual,
    /// `replace_combine_to_tev` fallback (TEXEL1 etc.).
    NotEv,
    /// Pre-converted TEV combiner (`G_SETCOMBINE_TEV`).
    Preconverted,
}

/// N64 combiner mux inputs (color).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MuxC {
    Combined,
    Texel0,
    Texel1,
    Primitive,
    Shade,
    Environment,
    One,
    Noise,
    Zero,
}

/// N64 combiner mux inputs (alpha).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MuxA {
    CombinedAlpha,
    Texel0Alpha,
    Texel1Alpha,
    PrimitiveAlpha,
    ShadeAlpha,
    EnvAlpha,
    One,
    Zero,
}

/// One TEV stage produced by the translation.
#[derive(Clone, Copy, Debug, Default)]
pub struct TevStage {
    pub color_in: [u8; 4], // a, b, c, d color inputs
    pub alpha_in: [u8; 4],
    pub color_op: u8,
    pub alpha_op: u8,
}

/// Result of translating one combiner expression.
#[derive(Clone, Debug, Default)]
pub struct TevProgram {
    pub stages: Vec<TevStage>,
    pub path: CombinePath,
}

/// A parsed N64 combiner: two cycles of (a,b,c,d) color + alpha muxes.
#[derive(Clone, Copy, Debug)]
pub struct CombinerExpr {
    pub cyc1: ([MuxC; 4], [MuxA; 4]),
    pub cyc2: Option<([MuxC; 4], [MuxA; 4])>,
}

impl CombinerExpr {
    /// `replace_combine_to_tev`: expressions that cannot use the simple
    /// mapping (any TEXEL1/TEXEL1_ALPHA mention) fall back to NOTEV.
    pub fn needs_notev(self) -> bool {
        let mentions_texel1 = |c: &[MuxC; 4], a: &[MuxA; 4]| {
            c.contains(&MuxC::Texel1) || a.contains(&MuxA::Texel1Alpha)
        };
        mentions_texel1(&self.cyc1.0, &self.cyc1.1)
            || self.cyc2.map(|(c, a)| mentions_texel1(&c, &a)).unwrap_or(false)
    }
}

/// `combine_auto`: compile the expression into TEV stages.
/// 1-cycle -> one/few stages; 2-cycle -> multiple stages.
pub fn combine_auto(expr: CombinerExpr, two_cycle: bool) -> TevProgram {
    if expr.needs_notev() {
        return TevProgram { stages: Vec::new(), path: CombinePath::NotEv };
    }
    let mut stages = Vec::new();
    stages.push(tev_stage_for(&expr.cyc1.0, &expr.cyc1.1));
    if two_cycle {
        if let Some((c, a)) = expr.cyc2 {
            stages.push(tev_stage_for(&c, &a));
        } else {
            stages.push(tev_stage_for(&expr.cyc1.0, &expr.cyc1.1));
        }
    }
    TevProgram { stages, path: CombinePath::Auto }
}

fn tev_stage_for(c: &[MuxC; 4], a: &[MuxA; 4]) -> TevStage {
    let map_c = |m: MuxC| match m {
        MuxC::Combined => 0,
        MuxC::Texel0 => 1,
        MuxC::Texel1 => 2,
        MuxC::Primitive => 3,
        MuxC::Shade => 4,
        MuxC::Environment => 5,
        MuxC::One => 6,
        MuxC::Noise => 7,
        MuxC::Zero => 8,
    };
    let map_a = |m: MuxA| match m {
        MuxA::CombinedAlpha => 0,
        MuxA::Texel0Alpha => 1,
        MuxA::Texel1Alpha => 2,
        MuxA::PrimitiveAlpha => 3,
        MuxA::ShadeAlpha => 4,
        MuxA::EnvAlpha => 5,
        MuxA::One => 6,
        MuxA::Zero => 7,
    };
    TevStage {
        color_in: [map_c(c[0]), map_c(c[1]), map_c(c[2]), map_c(c[3])],
        alpha_in: [map_a(a[0]), map_a(a[1]), map_a(a[2]), map_a(a[3])],
        color_op: 0,
        alpha_op: 0,
    }
}

/// Well-known combiner presets retail uses (for tests/docs).
pub mod presets {
    use super::*;

    /// `(0,0,0,TEXEL0),(0,0,0,1)`: modulate texture/primitive is the
    /// common textured-opaque preset.
    pub fn texel0_opaque() -> CombinerExpr {
        CombinerExpr {
            cyc1: ([MuxC::Zero, MuxC::Zero, MuxC::Zero, MuxC::Texel0],
                   [MuxA::Zero, MuxA::Zero, MuxA::Zero, MuxA::One]),
            cyc2: None,
        }
    }

    pub fn texel1_example() -> CombinerExpr {
        CombinerExpr {
            cyc1: ([MuxC::Zero, MuxC::Zero, MuxC::Zero, MuxC::Texel1],
                   [MuxA::Zero, MuxA::Zero, MuxA::Zero, MuxA::One]),
            cyc2: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texel1_falls_back_to_notev() {
        assert!(presets::texel1_example().needs_notev());
        assert!(!presets::texel0_opaque().needs_notev());
        let p = combine_auto(presets::texel1_example(), false);
        assert_eq!(p.path, CombinePath::NotEv);
    }

    #[test]
    fn two_cycle_produces_two_stages() {
        let p = combine_auto(presets::texel0_opaque(), true);
        assert_eq!(p.stages.len(), 2);
        let p1 = combine_auto(presets::texel0_opaque(), false);
        assert_eq!(p1.stages.len(), 1);
    }
}
