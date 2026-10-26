//! Paths across the navigation mesh: A* over linked cells, then
//! straightened wherever a straight line stays on the mesh.
//!
//! Cells on the edge of the walkable area (missing a link: a drop, a wall
//! or a gap beside them) cost more to cross, and a straightened line may
//! only cross cells that aren't on an edge. So a path keeps a cell away
//! from drops and walls where there's room, and hugs them only where it must.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::fmt;

use axcenity_core::Vector3;

use crate::{DIRECTIONS, NONE, NavMesh};

/// The extra cost, in cells, of crossing a cell on the edge of the walkable area.
const EDGE_COST: f32 = 2.0;

/// Why there is no path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathError {
    /// The start isn't on walkable ground.
    StartOffMesh,
    /// The destination isn't on walkable ground.
    DestinationOffMesh,
    /// Both are walkable, but no walkable route joins them.
    NotConnected,
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PathError::StartOffMesh => "the agent isn't standing on walkable ground",
            PathError::DestinationOffMesh => "that point isn't on walkable ground",
            PathError::NotConnected => "no walkable route leads there",
        })
    }
}

/// A cell waiting to be searched from, cheapest guess first.
#[derive(Clone, Copy, PartialEq)]
struct Open {
    /// The cost so far plus the guess of what's left.
    estimate: f32,
    cell: usize,
}

impl Eq for Open {}

impl Ord for Open {
    /// Reversed, because `BinaryHeap` pops the largest first.
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .estimate
            .total_cmp(&self.estimate)
            .then(other.cell.cmp(&self.cell))
    }
}

impl PartialOrd for Open {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl NavMesh {
    /// The corners of a walk from `from` to `to`, both included, each on
    /// the ground: `from` and `to` keep their X and Z, and every corner
    /// between is the middle of a cell. Consecutive corners are joined by
    /// ground a character can walk.
    pub fn find_path(&self, from: Vector3, to: Vector3) -> Result<Vec<Vector3>, PathError> {
        let start = self.cell_at(from).ok_or(PathError::StartOffMesh)?;
        let goal = self.cell_at(to).ok_or(PathError::DestinationOffMesh)?;
        if self.cells[start].region != self.cells[goal].region {
            return Err(PathError::NotConnected);
        }
        let cells = self.search(start, goal).ok_or(PathError::NotConnected)?;
        let on_ground =
            |cell: usize, p: Vector3| Vector3::new(p.x, self.ground_height(cell, p.x, p.z), p.z);
        let mut corners = vec![(start, on_ground(start, from))];
        if cells.len() > 2 {
            corners.extend(
                cells[1..cells.len() - 1]
                    .iter()
                    .map(|&cell| (cell, self.middle(cell))),
            );
        }
        corners.push((goal, on_ground(goal, to)));

        // Straighten: from each corner, go to the farthest one a straight
        // line reaches over the mesh.
        let mut path = vec![corners[0].1];
        let mut i = 0;
        while i + 1 < corners.len() {
            let mut j = i + 1;
            while j + 1 < corners.len() && self.straight(corners[i], corners[j + 1]) {
                j += 1;
            }
            path.push(corners[j].1);
            i = j;
        }
        Ok(path)
    }

    /// The height of the ground under `point`: on the cell a character at
    /// `point` stands on, following that cell's slope.
    pub fn ground_at(&self, point: Vector3) -> Option<f32> {
        let cell = self.cell_at(point)?;
        Some(self.ground_height(cell, point.x, point.z))
    }

    /// The cells from `start` to `goal`, both included, by A*.
    fn search(&self, start: usize, goal: usize) -> Option<Vec<usize>> {
        let target = &self.cells[goal];
        let guess = |cell: usize| {
            let cell = &self.cells[cell];
            (cell.column_x as f32 - target.column_x as f32).abs()
                + (cell.column_z as f32 - target.column_z as f32).abs()
        };
        // Only the cells reached are kept: a big terrain has millions.
        let mut cost: HashMap<usize, f32> = HashMap::from([(start, 0.0)]);
        let mut came_from: HashMap<usize, usize> = HashMap::new();
        let mut open = BinaryHeap::from([Open {
            estimate: guess(start),
            cell: start,
        }]);
        while let Some(Open { estimate, cell }) = open.pop() {
            if cell == goal {
                let mut cells = vec![goal];
                while let Some(&before) = came_from.get(cells.last().expect("never empty")) {
                    cells.push(before);
                }
                cells.reverse();
                return Some(cells);
            }
            let so_far = cost[&cell];
            // A cheaper way here was already searched from.
            if estimate > so_far + guess(cell) + 1e-4 {
                continue;
            }
            for next in self.links(cell) {
                let step = if self.is_interior(next) {
                    1.0
                } else {
                    1.0 + EDGE_COST
                };
                let reached = so_far + step;
                if cost.get(&next).is_none_or(|&known| reached < known) {
                    cost.insert(next, reached);
                    came_from.insert(next, cell);
                    open.push(Open {
                        estimate: reached + guess(next),
                        cell: next,
                    });
                }
            }
        }
        None
    }

    /// True when a straight line from `a` (on cell `from`) to `b` (on cell
    /// `to`) crosses only linked cells, none of them on an edge except
    /// `from` and `to`, and ends on `to`. Where the line passes exactly
    /// through a corner, both ways round it must be linked.
    fn straight(&self, (from, a): (usize, Vector3), (to, b): (usize, Vector3)) -> bool {
        let size = self.settings.cell_size;
        let (u0, v0) = ((a.x - self.origin_x) / size, (a.z - self.origin_z) / size);
        let (u1, v1) = ((b.x - self.origin_x) / size, (b.z - self.origin_z) / size);
        let (du, dv) = (u1 - u0, v1 - v0);
        let (mut cx, mut cz) = (u0.floor() as i64, v0.floor() as i64);
        let end = (u1.floor() as i64, v1.floor() as i64);
        // Directions as indexes into DIRECTIONS: -X, +X, -Z, +Z.
        let (step_x, dir_x) = if du > 0.0 { (1, 1) } else { (-1, 0) };
        let (step_z, dir_z) = if dv > 0.0 { (1, 3) } else { (-1, 2) };
        // How far along the line (0 to 1) the next X and Z column borders
        // are, and the distance between borders.
        let first_border = |start: f32, cell: i64, d: f32| {
            if d > 0.0 {
                (cell as f32 + 1.0 - start) / d
            } else if d < 0.0 {
                (cell as f32 - start) / d
            } else {
                f32::INFINITY
            }
        };
        let mut t_x = first_border(u0, cx, du);
        let mut t_z = first_border(v0, cz, dv);
        let gap_x = if du == 0.0 {
            f32::INFINITY
        } else {
            1.0 / du.abs()
        };
        let gap_z = if dv == 0.0 {
            f32::INFINITY
        } else {
            1.0 / dv.abs()
        };
        let mut left = (end.0 - cx).abs() + (end.1 - cz).abs() + 2;
        let mut cell = from;
        while (cx, cz) != end {
            if left == 0 || (cell != from && !self.is_interior(cell)) {
                return false;
            }
            left -= 1;
            if (t_x - t_z).abs() < 1e-5 {
                let round_x = self.link(cell, dir_x).filter(|&c| self.is_interior(c));
                let round_z = self.link(cell, dir_z).filter(|&c| self.is_interior(c));
                let via_x = round_x.and_then(|c| self.link(c, dir_z));
                let via_z = round_z.and_then(|c| self.link(c, dir_x));
                match (via_x, via_z) {
                    (Some(p), Some(q)) if p == q => cell = p,
                    _ => return false,
                }
                cx += step_x;
                cz += step_z;
                t_x += gap_x;
                t_z += gap_z;
            } else if t_x < t_z {
                let Some(next) = self.link(cell, dir_x) else {
                    return false;
                };
                cell = next;
                cx += step_x;
                t_x += gap_x;
            } else {
                let Some(next) = self.link(cell, dir_z) else {
                    return false;
                };
                cell = next;
                cz += step_z;
                t_z += gap_z;
            }
        }
        cell == to
    }

    /// The cell linked to `cell` toward [`DIRECTIONS`]`[direction]`.
    fn link(&self, cell: usize, direction: usize) -> Option<usize> {
        let link = self.cells[cell].links[direction];
        (link != NONE).then_some(link as usize)
    }

    /// True when the cell is linked all four ways: not on an edge.
    fn is_interior(&self, cell: usize) -> bool {
        self.cells[cell].links.iter().all(|&link| link != NONE)
    }

    /// The middle of a cell, on its ground.
    fn middle(&self, cell: usize) -> Vector3 {
        let c = &self.cells[cell];
        let (x, z) = self.centre(c.column_x as usize, c.column_z as usize);
        Vector3::new(x, c.height, z)
    }

    /// The ground's height at `x`, `z` on `cell`, along the cell's slope.
    fn ground_height(&self, cell: usize, x: f32, z: f32) -> f32 {
        let c = &self.cells[cell];
        let (cx, cz) = self.centre(c.column_x as usize, c.column_z as usize);
        let half = self.settings.cell_size * 0.5;
        let (dx, dz) = ((x - cx).clamp(-half, half), (z - cz).clamp(-half, half));
        let up = c.normal.y.max(0.05);
        c.height - (c.normal.x * dx + c.normal.z * dz) / up
    }
}

/// Kept in step with `DIRECTIONS`' order, which `straight` relies on.
const _: () = assert!(
    DIRECTIONS[0].0 == -1 && DIRECTIONS[1].0 == 1 && DIRECTIONS[2].1 == -1 && DIRECTIONS[3].1 == 1
);
