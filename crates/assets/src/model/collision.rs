//! Collision pieces (roadmap R15.2): objects in a model that say what it
//! collides as instead of being drawn.
//!
//! An object named like another with `_COL` on the end (or `_COL1`,
//! `_COL2` …, in any case) is one convex piece of that object's collision:
//! the convex hull of its corners. Everything inside such an object is part
//! of the same piece. Pieces aren't drawn and don't size the model; they're
//! fitted into its 1×1×1 cube the same way as its triangles
//! ([`super::Model::collision`]), and a MeshPart whose `CollisionFidelity`
//! is Default collides as them (`physics`).

/// The length of the name `name` is a piece of (`Rock` in `Rock_COL2`),
/// or `None` if it isn't a piece.
fn owner_len(name: &str) -> Option<usize> {
    let name = name.trim_end();
    let without_number = name.trim_end_matches(|c: char| c.is_ascii_digit());
    // ASCII lowercasing keeps every byte where it was.
    without_number
        .to_ascii_lowercase()
        .ends_with("_col")
        .then(|| without_number.len() - "_col".len())
}

/// True if an object named `name` is a collision piece: its name ends with
/// `_COL`, or `_COL` and a number, in any case.
pub fn is_piece(name: &str) -> bool {
    owner_len(name).is_some()
}

/// The path of the object the piece at `path` belongs to (`Rock` for
/// `Rock_COL1`), or `None` if it isn't a piece.
pub(super) fn owner_path(path: &str) -> Option<&str> {
    let path = path.trim_end();
    owner_len(path).map(|len| &path[..len])
}

/// The pieces met while reading a file, each the corners of its meshes in
/// the file's space.
#[derive(Default)]
pub(super) struct Pieces {
    pub list: Vec<Vec<[f32; 3]>>,
    /// The piece the objects being read belong to.
    current: Option<usize>,
}

impl Pieces {
    /// Called on entering an object named `name`: a piece starts there,
    /// unless it's inside one already. Returns what [`Pieces::leave`] puts
    /// back.
    pub fn enter(&mut self, name: &str) -> Option<usize> {
        let outer = self.current;
        if outer.is_none() && is_piece(name) {
            self.list.push(Vec::new());
            self.current = Some(self.list.len() - 1);
        }
        outer
    }

    /// Called on leaving the object [`Pieces::enter`] returned `outer` for.
    pub fn leave(&mut self, outer: Option<usize>) {
        self.current = outer;
    }

    /// True while the objects being read are a piece's, not drawn.
    pub fn reading(&self) -> bool {
        self.current.is_some()
    }

    /// Adds corners to the piece being read.
    pub fn add(&mut self, corners: impl IntoIterator<Item = [f32; 3]>) {
        if let Some(piece) = self.current.and_then(|index| self.list.get_mut(index)) {
            piece.extend(corners);
        }
    }

    /// The pieces read, leaving none.
    pub fn take(&mut self) -> Vec<Vec<[f32; 3]>> {
        self.current = None;
        std::mem::take(&mut self.list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pieces_are_named_with_col_and_maybe_a_number() {
        for name in [
            "Rock_COL",
            "Rock_col",
            "Rock_COL1",
            "Rock_Col12",
            "Rock_COL ",
        ] {
            assert!(is_piece(name), "{name}");
            assert_eq!(owner_path(name), Some("Rock"), "{name}");
        }
        for name in [
            "Rock",
            "Rock_LOD1",
            "RockCOL",
            "Rock_COLLAR",
            "Rock_COL_A",
            "Collar",
        ] {
            assert!(!is_piece(name), "{name}");
        }
        assert_eq!(owner_path("Rocks/Big_COL2"), Some("Rocks/Big"));
    }

    #[test]
    fn everything_inside_a_piece_is_part_of_it() {
        let mut pieces = Pieces::default();
        let outer = pieces.enter("Wall_COL");
        pieces.add([[0.0; 3]]);
        // A child named like a piece doesn't start another.
        let inner = pieces.enter("Wall_COL1");
        pieces.add([[1.0; 3]]);
        pieces.leave(inner);
        pieces.leave(outer);
        assert!(!pieces.reading());
        pieces.add([[2.0; 3]]);
        assert_eq!(pieces.take(), vec![vec![[0.0; 3], [1.0; 3]]]);
    }
}
