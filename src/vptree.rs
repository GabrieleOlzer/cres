use std::cmp::PartialOrd;
use std::default::Default;
use std::iter::FromIterator;
use std::ops::Sub;

use log::{debug, trace};
use num_traits::sign::Signed;

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct VPTree<P, D> {
    nodes: Vec<Node<P, D>>,
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
struct Node<P, D> {
    vantage_pt: P,
    children: Option<Children<D>>
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
struct Children<D> {
    radius: D,
    outside_offset: usize,
}

pub trait Dist<P = Self> {
    type Output: Copy + Default + PartialOrd + Signed + Sub;

    fn dist(&self, p: &P) -> Self::Output;
}

impl<P: Dist> VPTree<P, <P as Dist>::Output> {
    pub fn new(nodes: Vec<P>) -> Self {
        Self::from_iter(nodes.into_iter())
    }

    pub fn nearest(&self, pt: &P) -> Option<(&P, <P as Dist>::Output)> {
        self.nearest_in(pt, |p, q| p.dist(q))
    }

    pub fn filtered_nearest<F>(
        &self,
        pt: &P,
        filter: F,
    ) -> Option<(&P, <P as Dist>::Output)>
    where
        F: FnMut(&P) -> bool
    {
        self.filtered_nearest_in(pt, |p, q| p.dist(q), filter)
    }
}

impl<P: Dist> FromIterator<P> for VPTree<P, <P as Dist>::Output> {
    fn from_iter<I: IntoIterator<Item=P>>(iter: I) -> Self {
        Self::from_iter_with_dist(iter, |p, q| p.dist(q))
    }
}

impl<P, D: Copy + Default + PartialOrd + Signed + Sub> VPTree<P, D> {
    pub fn new_with_dist<DF>(
        nodes: Vec<P>,
        dist: DF
    ) -> Self
    where
        for<'a, 'b> DF: FnMut(&'a P, &'b P) -> D
    {
        Self::from_iter_with_dist(nodes.into_iter(), dist)
    }
}

impl<'x, P: 'x, D: Copy + Default + PartialOrd + Signed + Sub> VPTree<P, D> {
    pub fn from_iter_with_dist<DF, I>(
        iter: I,
        mut dist: DF
    ) -> Self
    where
        I: IntoIterator<Item = P>,
        for<'a, 'b> DF: FnMut(&'a P, &'b P) -> D
    {
        let mut nodes = Vec::from_iter(
            iter.into_iter().map(
                |vantage_pt| {
                    // reserve first element for storing distances
                    (Default::default(), Node{ vantage_pt, children: None })
                }
            )
        );
        let corner_pt_idx = Self::find_corner_pt(
            nodes.iter().map(|(_, pt)| &pt.vantage_pt),
            &mut dist
        );
        debug!("first vantage point: {corner_pt_idx:?}");
        if let Some(pos) = corner_pt_idx {
            let last_idx = nodes.len() - 1;
            nodes.swap(pos, last_idx)
        }
        Self::build_tree(nodes.as_mut_slice(), &mut dist);
        let nodes = nodes.into_iter().map(|(_d, n)| n).collect();
        Self { nodes }
    }

    fn find_corner_pt<'a, I, DF>(
        iter: I,
        dist: &mut DF
    ) -> Option<usize>
    where
        'x: 'a,
        I: IntoIterator<Item = &'a P>,
        for<'b, 'c> DF: FnMut(&'b P, &'c P) -> D
    {
        let mut iter = iter.into_iter();
        if let Some(first) = iter.next() {
            let max = iter.enumerate().max_by(
                |(_, a), (_, b)| dist(&first, a).partial_cmp(&dist(&first, b)).unwrap()
            );
            if let Some((pos, _)) = max {
                Some(pos + 1)
            } else {
                Some(0)
            }
        } else {
            None
        }
    }

    // Recursively build the vantage point tree
    //
    // 1. Choose the point with the largest distance to the parent as
    //    the next vantage point. The initial distances are chosen
    //    with respect to an arbitrary point, so the first vantage
    //    point is in some corner of space.
    //
    // 2. Calculate the distances of all other points to the vantage
    //    point.
    //
    // 3. Define the "inside" set as the points within less than the
    //    median distance to the vantage point, excepting the vantage
    //    point itself. The points with larger distance form the
    //    "outside" set. Build vantage point trees for each of the two
    //    sets.
    //
    fn build_tree<DF>(
        pts: &mut [(D, Node<P, D>)],
        dist: &mut DF,
    )
    where
        for<'a, 'b> DF: FnMut(&'a P, &'b P) -> D
    {
        if pts.len() < 2 { return }
        // debug_assert!(pts.is_sorted_by_key(|pt| pt.0))
        // the last point is the one furthest away from the parent,
        // so it is the best candidate for the next vantage point
        pts.swap(0, pts.len() - 1);
        let (vp, pts) = pts.split_first_mut().unwrap();
        for (d, pt) in pts.iter_mut() {
            *d = dist(&vp.1.vantage_pt, &pt.vantage_pt)
        }
        pts.sort_unstable_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let median_idx = pts.len() / 2;
        let (inside, outside) = pts.split_at_mut(median_idx);
        vp.1.children = Some(Children {
            radius: outside.first().unwrap().0,
            outside_offset: median_idx
        });
        Self::build_tree(inside, dist);
        Self::build_tree(outside, dist);
    }

    pub fn nearest_in<DF, Q>(&self, pt: &Q, mut dist: DF) -> Option<(&P, D)>
    where
        for<'a, 'b> DF: FnMut(&'a Q, &'b P) -> D
    {
        debug!("Starting nearest neighbour search");
        Self::filtered_nearest_in_subtree(
            self.nodes.as_slice(),
            pt,
            &mut dist,
            &mut |_| true
        )
    }

    pub fn filtered_nearest_in<DF, F, Q>(
        &self,
        pt: &Q,
        mut dist: DF,
        mut filter: F,
    ) -> Option<(&P, D)>
    where
        for<'a, 'b> DF: FnMut(&'a Q, &'b P) -> D,
        F: FnMut(&P) -> bool,
    {
        debug!("Starting nearest neighbour search");
        Self::filtered_nearest_in_subtree(
            self.nodes.as_slice(),
            pt,
            &mut dist,
            &mut filter
        )
    }

    fn filtered_nearest_in_subtree<'a, DF, F, Q>(
        subtree: &'a [Node<P, D>],
        pt: &Q,
        dist: &mut DF,
        filter: &mut F,
    ) -> Option<(&'a P, D)>
    where
        for<'b, 'c> DF: FnMut(&'b Q, &'c P) -> D,
        F: FnMut(&P) -> bool,
    {
        if let Some((vp, tree)) = subtree.split_first() {
            let d = dist(pt, &vp.vantage_pt);
            let mut nearest = if filter(&vp.vantage_pt) {
                Some((&vp.vantage_pt, d))
            } else {
                None
            };
            if let Some(children) = &vp.children {
                let mut subtrees = tree.split_at(children.outside_offset);
                let mut nearest_in_sub = |sub| Self::filtered_nearest_in_subtree(
                    sub,
                    pt,
                    dist,
                    filter,
                );
                if d > children.radius {
                    std::mem::swap(&mut subtrees.0, &mut subtrees.1);
                    trace!("Looking into outer region first");
                }
                trace!("Looking for nearest neighbour in more promising region");
                nearest = Self::nearer(nearest, nearest_in_sub(subtrees.0));
                if let Some((_, dn)) = nearest {
                    if dn < (children.radius - d).abs() {
                        return nearest;
                    }
                }
                trace!("Looking for nearest neighbour in less promising region");
                Self::nearer(nearest, nearest_in_sub(subtrees.1))
            } else {
                nearest
            }
        } else {
            None
        }
    }

    fn nearer<'a>(a: Option<(&'a P, D)>, b: Option<(&'a P, D)>) -> Option<(&'a P, D)> {
        match (a, b) {
            (Some((_, d1)), Some((_, d2))) => if d1 <= d2 {
                a
            } else {
                b
            },
            (None, Some(_)) => b,
            _ => a,
        }
    }
}

impl<P: Copy + Default + PartialOrd + Signed + Sub<Output = P>> Dist for P {
    type Output = Self;

    fn dist(&self, p: &P) -> Self::Output {
        (*self - *p).abs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::debug;

    fn log_init() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    #[test]
    fn nearest() {
        log_init();

        let tree = VPTree::<i32, i32>::from_iter([]);
        debug!("{tree:#?}");
        assert_eq!(tree.nearest(&0), None);

        let tree = VPTree::from_iter([0]);
        debug!("{tree:#?}");
        assert_eq!(tree.nearest(&-1), Some((&0, 1)));
        assert_eq!(tree.nearest(&0), Some((&0, 0)));
        assert_eq!(tree.nearest(&1), Some((&0, 1)));

        let tree = VPTree::from_iter([0, 1]);
        debug!("{tree:#?}");
        assert_eq!(tree.nearest(&0), Some((&0, 0)));
        assert_eq!(tree.nearest(&1), Some((&1, 0)));
        assert_eq!(tree.nearest(&2), Some((&1, 1)));

        let tree = VPTree::from_iter([0, 1, 4]);
        debug!("{tree:#?}");
        assert_eq!(tree.nearest(&3), Some((&4, 1)));

        let tree = VPTree::from_iter([0, 1, 2, 3]);
        debug!("{tree:#?}");
        assert_eq!(tree.nearest(&2), Some((&2, 0)));
        assert_eq!(tree.nearest(&5), Some((&3, 2)));
        assert_eq!(tree.nearest(&-5), Some((&0, 5)));
    }

    #[test]
    fn nearest_filtered() {
        log_init();

        let tree = VPTree::<i32, i32>::from_iter([0]);
        debug!("{tree:#?}");
        assert_eq!(tree.filtered_nearest(&-1, |p| *p != 0), None);
        assert_eq!(tree.filtered_nearest(&-1, |p| *p == 0), Some((&0, 1)));

        let tree = VPTree::<i32, i32>::from_iter([0, 1]);
        debug!("{tree:#?}");
        assert_eq!(tree.filtered_nearest(&0, |p| *p > 0), Some((&1, 1)));
    }

}
