//! The chisel rules (ADR-0008): what a chisel may do to a block, and the
//! shape it leaves. The server checks an edit with these (`check_edit`) and
//! the client predicts with the same ones, so the two never disagree on a
//! rule or a word. Checks that need server state (reach, claims, rate, wear,
//! a player in the way of a fill) are the server's.

use crate::block::{BlockDef, BlockState, Chiselling, Shape};
use crate::shape::{ShapeFamily, edge_pairs};

/// Why a chisel cannot act: words for the player, or a reason only worth
/// logging (a client that aims honestly never sends these).
#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    Quiet(&'static str),
    Told(String),
}

/// The shape a chisel of `family` (`precise` or not) leaves on `state`, whose
/// catalogue entry is `def`, after cutting `octants` away, or with `restore`,
/// filling them back.
#[allow(clippy::too_many_arguments)]
pub fn apply(
    def: &BlockDef,
    state: BlockState,
    family: ShapeFamily,
    precise: bool,
    advanced: bool,
    octants: u8,
    restore: bool,
) -> Result<Shape, Refused> {
    if state.id.is_air() || !def.solid {
        return Err(Refused::Quiet("nothing to chisel"));
    }
    if def.hard && !advanced {
        return Err(Refused::Told(format!(
            "{} is too hard: it takes an advanced chisel",
            def.display_name()
        )));
    }
    if !def.chisel.allows(family) {
        return Err(Refused::Told(match def.chisel {
            Chiselling::Slope => "Natural ground takes only slope chisels".into(),
            _ => format!("{} cannot be chiselled", def.display_name()),
        }));
    }
    // Precise chisels move one octant, ordinary ones an edge's two.
    let fits = if precise {
        octants.count_ones() == 1
    } else {
        edge_pairs().any(|m| m == octants)
    };
    if !fits {
        return Err(Refused::Quiet("not a cut this chisel makes"));
    }
    if state.shape.is_form() {
        return Err(Refused::Told(format!("{} cannot be chiselled", def.display_name())));
    }
    let cut = if state.shape.is_valid() {
        state.shape
    } else {
        Shape::FULL
    };
    if cut.family().is_some_and(|f| f != family) {
        return Err(Refused::Told(
            "This block is already cut by another kind of chisel".into(),
        ));
    }
    if restore {
        if octants & !cut.removed() != 0 {
            return Err(Refused::Quiet("nothing cut there to fill"));
        }
        Ok(cut.restored(octants))
    } else {
        if octants & cut.removed() != 0 {
            return Err(Refused::Quiet("already cut there"));
        }
        cut.chiselled(family, octants)
            .ok_or_else(|| Refused::Told("That would cut the whole block away".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::BlockRegistry;
    use crate::shape::octant;

    const EDGE: u8 = 1 << octant(1, 1, 0) | 1 << octant(1, 1, 1);

    fn block<'a>(r: &'a BlockRegistry, name: &str) -> (&'a BlockDef, BlockState) {
        let id = r.id(name).unwrap();
        (r.get(id), BlockState::plain(id))
    }

    fn told(r: Result<Shape, Refused>) -> String {
        match r {
            Err(Refused::Told(t)) => t,
            other => panic!("expected words for the player, got {other:?}"),
        }
    }

    #[test]
    fn hard_blocks_take_only_advanced_chisels() {
        let r = BlockRegistry::builtin();
        for name in ["hard_coal_seam", "diamond_seam", "ruby_seam"] {
            let (def, seam) = block(&r, name);
            assert!(def.hard, "{name} is hard");
            assert!(told(apply(def, seam, ShapeFamily::Slope, false, false, EDGE, false)).contains("too hard"));
            let advanced = apply(def, seam, ShapeFamily::Slope, false, true, EDGE, false);
            assert!(
                !matches!(&advanced, Err(Refused::Told(t)) if t.contains("too hard")),
                "{name}: {advanced:?}"
            );
        }
        let (def, stone) = block(&r, "stone");
        assert!(!def.hard);
        assert!(apply(def, stone, ShapeFamily::Slope, false, false, EDGE, false).is_ok());
    }

    #[test]
    fn natural_ground_takes_slope_chisels_only() {
        let r = BlockRegistry::builtin();
        let (def, stone) = block(&r, "stone");
        assert_eq!(def.chisel, Chiselling::Slope);
        for family in [ShapeFamily::Square, ShapeFamily::Bevel, ShapeFamily::Round] {
            assert_eq!(
                told(apply(def, stone, family, false, false, EDGE, false)),
                "Natural ground takes only slope chisels"
            );
        }
        assert_eq!(
            apply(def, stone, ShapeFamily::Slope, false, false, EDGE, false),
            Ok(Shape::new(ShapeFamily::Slope, EDGE).unwrap())
        );
    }

    #[test]
    fn a_block_keeps_the_family_of_its_first_cut() {
        let r = BlockRegistry::builtin();
        let (def, brick) = block(&r, "brick");
        let cut = BlockState {
            shape: apply(def, brick, ShapeFamily::Square, false, false, EDGE, false).unwrap(),
            ..brick
        };
        for family in [ShapeFamily::Slope, ShapeFamily::Bevel, ShapeFamily::Round] {
            assert_eq!(
                told(apply(def, cut, family, true, false, 1, false)),
                "This block is already cut by another kind of chisel"
            );
        }
        let more = apply(def, cut, ShapeFamily::Square, true, false, 1, false).unwrap();
        assert_eq!(more, Shape::new(ShapeFamily::Square, EDGE | 1).unwrap());
    }

    #[test]
    fn precise_chisels_move_one_octant_and_ordinary_ones_an_edge_pair() {
        let r = BlockRegistry::builtin();
        let (def, brick) = block(&r, "brick");
        let fam = ShapeFamily::Bevel;
        for o in 0..8 {
            assert!(apply(def, brick, fam, true, false, 1 << o, false).is_ok());
            assert_eq!(
                apply(def, brick, fam, false, false, 1 << o, false),
                Err(Refused::Quiet("not a cut this chisel makes"))
            );
        }
        for m in edge_pairs() {
            assert!(apply(def, brick, fam, false, false, m, false).is_ok());
            assert!(apply(def, brick, fam, true, false, m, false).is_err());
        }
        // Two octants that share no edge are no chisel's cut.
        let across = 1 << octant(0, 0, 0) | 1 << octant(1, 1, 1);
        assert!(apply(def, brick, fam, false, false, across, false).is_err());
        assert!(apply(def, brick, fam, false, false, 0, false).is_err());
    }

    #[test]
    fn machines_ores_plants_and_liquids_refuse() {
        let r = BlockRegistry::builtin();
        for (name, title) in [
            ("workbench", "Workbench"),
            ("copper_vein", "Copper Vein"),
            ("foliage", "Foliage"),
        ] {
            let (def, state) = block(&r, name);
            assert_eq!(
                told(apply(def, state, ShapeFamily::Slope, true, false, 1, false)),
                format!("{title} cannot be chiselled")
            );
        }
        for name in ["water", "air"] {
            let (def, state) = block(&r, name);
            assert_eq!(
                apply(def, state, ShapeFamily::Square, true, false, 1, false),
                Err(Refused::Quiet("nothing to chisel"))
            );
        }
    }

    #[test]
    fn cuts_fill_back_to_the_full_block() {
        let r = BlockRegistry::builtin();
        let (def, brick) = block(&r, "brick");
        let fam = ShapeFamily::Square;
        let cut = BlockState {
            shape: Shape::new(fam, EDGE).unwrap(),
            ..brick
        };
        assert_eq!(
            apply(def, cut, fam, false, false, EDGE, false),
            Err(Refused::Quiet("already cut there"))
        );
        assert_eq!(
            apply(def, cut, fam, true, false, 1 << octant(1, 1, 0), true),
            Ok(Shape::new(fam, 1 << octant(1, 1, 1)).unwrap())
        );
        assert_eq!(apply(def, cut, fam, false, false, EDGE, true), Ok(Shape::FULL));
        assert_eq!(
            apply(def, brick, fam, true, false, 1, true),
            Err(Refused::Quiet("nothing cut there to fill"))
        );
        // The last octant cannot go: that is breaking, not chiselling.
        let one_left = BlockState {
            shape: Shape::new(fam, !1).unwrap(),
            ..brick
        };
        assert_eq!(
            told(apply(def, one_left, fam, true, false, 1, false)),
            "That would cut the whole block away"
        );
    }
}
