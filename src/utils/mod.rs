//! Helper functions that poisson uses.

use {Builder, Type, Vector, Float};

use num::NumCast;

use rand::{Rand, Rng};

use modulo::Mod;

use std::marker::PhantomData;

pub mod math;

#[derive(Clone)]
pub struct Grid<F, V>
    where F: Float,
          V: Vector<F>
{
    data: Vec<Vec<V>>,
    side: usize,
    cell: F,
    poisson_type: Type,
    _marker: PhantomData<F>,
}

impl<F, V> Grid<F, V>
    where F: Float,
          V: Vector<F>
{
    pub fn new(radius: F, poisson_type: Type) -> Grid<F, V> {
        let dim = F::cast(V::dimension(None));
        let cell = (F::cast(2) * radius) / dim.sqrt();
        let side = (F::cast(1) / cell)
                       .to_usize()
                       .expect("Expected that dividing 1 by cell width would be legal.");
        Grid {
            cell: cell,
            side: side,
            data: vec![vec![]; side.pow(dim.to_u32().expect("Dimension should be always be castable to u32."))],
            poisson_type: poisson_type,
            _marker: PhantomData,
        }
    }

    pub fn get(&self, index: V) -> Option<&Vec<V>> {
        encode(&index, self.side, self.poisson_type).map(|t| &self.data[t])
    }

    pub fn get_mut(&mut self, index: V) -> Option<&mut Vec<V>> {
        encode(&index, self.side, self.poisson_type).map(move |t| &mut self.data[t])
    }

    pub fn cells(&self) -> usize {
        self.data.len()
    }

    pub fn side(&self) -> usize {
        self.side
    }

    pub fn cell(&self) -> F {
        self.cell
    }
}

pub fn encode<F, V>(v: &V, side: usize, poisson_type: Type) -> Option<usize>
    where F: Float,
          V: Vector<F>
{
    use Type::*;
    let mut index = 0;
    for &n in v.iter() {
        let cur = match poisson_type {
            Perioditic => {
                n.to_isize()
                 .expect("Expected that all scalars of the index vector should be castable to \
                          isize.")
                 .modulo(side as isize) as usize
            }
            Normal => {
                if n < F::cast(0) || n >= F::cast(side) {
                    return None;
                }
                n.to_usize()
                 .expect("Expected that all scalars of the index vector should be castable to \
                          usize.")
            }
        };
        index = (index + cur) * side;
    }
    Some(index / side)
}

pub fn decode<F, V>(index: usize, side: usize) -> Option<V>
    where F: Float,
          V: Vector<F>
{
    use num::Zero;
    let dim = V::dimension(None);
    if index >= side.pow(dim as u32) {
        return None;
    }
    let mut result = V::zero();
    let mut last = index;
    for n in result.iter_mut().rev() {
        let cur = last / side;
        *n = F::cast(last - cur * side);
        last = cur;
    }
    Some(result)
}

#[test]
fn encoding_decoding_works() {
    let n = ::na::Vector2::new(10., 7.);
    assert_eq!(n,
               decode(encode(&n, 15, Type::Normal).unwrap(), 15).unwrap());
}

#[test]
fn encoding_decoding_at_edge_works() {
    let n = ::na::Vector2::new(14., 14.);
    assert_eq!(n,
               decode(encode(&n, 15, Type::Normal).unwrap(), 15).unwrap());
}

#[test]
fn encoding_outside_of_area_fails() {
    let n = ::na::Vector2::new(9., 7.);
    assert_eq!(None, encode(&n, 9, Type::Normal));
    let n = ::na::Vector2::new(7., 9.);
    assert_eq!(None, encode(&n, 9, Type::Normal));
}

#[test]
fn decoding_outside_of_area_fails() {
    assert_eq!(None, decode::<f64, ::na::Vector2<_>>(100, 10));
}

pub fn choose_random_sample<F, V, R>(rng: &mut R, grid: &Grid<F, V>, index: V, level: usize) -> V
    where F: Float,
          V: Vector<F>,
          R: Rng
{
    let side = 2usize.pow(level as u32);
    let spacing = grid.cell / F::cast(side);
    (index + V::rand(rng)) * spacing
}

#[test]
fn random_point_is_between_right_values_top_lvl() {
    use num::Zero;
    use rand::{SeedableRng, XorShiftRng};
    use na::Vector2 as Vec2;
    let mut rand = XorShiftRng::from_seed([1, 2, 3, 4]);
    let radius = 0.2;
    let grid = Grid::<f64, Vec2<_>>::new(radius, Type::Normal);
    for _ in 0..1000 {
        let result = choose_random_sample(&mut rand, &grid, Vec2::<f64>::zero(), 0);
        assert!(result.x >= 0.);
        assert!(result.x < grid.cell);
        assert!(result.y >= 0.);
        assert!(result.y < grid.cell);
    }
}

pub fn sample_to_index<F, V>(value: &V, side: usize) -> V
    where F: Float,
          V: Vector<F>
{
    let mut cur = value.clone();
    for c in cur.iter_mut() {
        *c = (*c * F::cast(side)).floor();
    }
    cur
}

pub fn index_to_sample<F, V>(value: &V, side: usize) -> V
    where F: Float,
          V: Vector<F>
{
    let mut cur = value.clone();
    for c in cur.iter_mut() {
        *c = *c / F::cast(side);
    }
    cur
}

#[cfg(test)]
quickcheck! {
    fn index_prop(x: f64, y: f64, max: usize) -> bool {
        if !(0. <= x && x < 1. && 0. <= y && y < 1.) {
            return true;
        }
        if max == 0 {
            return true;
        }
        let xs = ::na::Vector2::new(x, y);
        xs == index_to_sample(&sample_to_index(&xs, max), max)
    }
}

pub fn is_disk_free<F, V>(grid: &Grid<F, V>,
                          poisson: &Builder<F, V>,
                          index: V,
                          level: usize,
                          sample: V,
                          outside: &[V])
                          -> bool
    where F: Float,
          V: Vector<F>
{
    let parent = get_parent(index, level);
    let sqradius = (F::cast(2) * poisson.radius).powi(2);
    // NOTE: This does unnessary checks for corners, but it doesn't affect much in higher dimensions: 5^d vs 5^d - 2d
    each_combination(&[-2, -1, 0, 1, 2])
        .filter_map(|t| grid.get(parent.clone() + t))
        .flat_map(|t| t)
        .all(|v| sqdist(v.clone(), sample.clone(), poisson.poisson_type) >= sqradius) &&
    is_valid(poisson, outside, sample)
}

pub fn is_valid<F, V>(poisson: &Builder<F, V>, samples: &[V], sample: V) -> bool
    where F: Float,
          V: Vector<F>
{
    let sqradius = (F::cast(2) * poisson.radius).powi(2);
    samples.iter()
           .all(|t| sqdist(t.clone(), sample.clone(), poisson.poisson_type) >= sqradius)
}


pub fn sqdist<F, V>(v1: V, v2: V, poisson_type: Type) -> F
    where F: Float,
          V: Vector<F>
{
    use Type::*;
    let diff = v2 - v1;
    match poisson_type {
        Perioditic => {
            each_combination(&[-1, 0, 1])
                .map(|v| (diff.clone() + v).norm_squared())
                .fold(F::max_value(), |a, b| a.min(b))
        }
        Normal => diff.norm_squared(),
    }
}

pub fn get_parent<F, V>(mut index: V, level: usize) -> V
    where F: Float,
          V: Vector<F>
{
    let split = 2usize.pow(level as u32);
    for n in index.iter_mut() {
        *n = (*n / F::cast(split)).floor();
    }
    index
}

#[test]
fn getting_parent_works() {
    let divides = 4;
    let cells_per_cell = 2usize.pow(divides as u32);
    let testee = ::na::Vector2::new(1., 2.);
    assert_eq!(testee,
               get_parent((testee * cells_per_cell as f64) + ::na::Vector2::new(0., 15.),
                          divides));
}

pub struct CombiIter<'a, F, FF, V>
    where F: Float,
          FF: NumCast + 'a,
          V: Vector<F>
{
    cur: usize,
    choices: &'a [FF],
    _marker: PhantomData<(F, V)>,
}

impl<'a, F, FF, V> Iterator for CombiIter<'a, F, FF, V>
    where F: Float,
          FF: NumCast + Clone,
          V: Vector<F>
{
    type Item = V;
    fn next(&mut self) -> Option<Self::Item> {
        let dim = V::dimension(None);
        let len = self.choices.len();
        if self.cur >= len.pow(dim as u32) {
            None
        } else {
            let mut result = V::zero();
            let mut div = self.cur;
            self.cur += 1;
            for n in result.iter_mut() {
                let rem = div % len;
                div /= len;
                let choice = self.choices[rem as usize].clone();
                *n = NumCast::from(choice)
                         .expect("Expected that all choices were castable to float without \
                                  problems.");
            }
            Some(result)
        }
    }
}

/// Iterates through all combinations of vectors with allowed values as scalars.
pub fn each_combination<'a, F, FF, V>(choices: &[FF]) -> CombiIter<F, FF, V>
    where F: Float + 'a,
          FF: NumCast,
          V: Vector<F>
{
    CombiIter {
        cur: 0,
        choices: choices,
        _marker: PhantomData,
    }
}

/// Trait that allows flat mapping inplace.
pub trait Inplace<T> {
    /// Does flat map inplace without maintaining order of elements.
    fn flat_map_inplace<F, I>(&mut self, f: F)
        where I: IntoIterator<Item = T>,
              F: FnMut(T) -> I;
}

impl<T> Inplace<T> for Vec<T> {
    fn flat_map_inplace<F, I>(&mut self, mut f: F)
        where I: IntoIterator<Item = T>,
              F: FnMut(T) -> I
    {
        for i in (0..self.len()).rev() {
            for t in f(self.swap_remove(i)) {
                self.push(t);
            }
        }
    }
}

#[test]
fn mapping_inplace_works() {
    let vec = vec![1, 2, 3, 4, 5, 6];
    let mut result = vec.clone();
    let func = |t| {
        match t % 3 {
            0 => (0..0),
            1 => (0..1),
            _ => (0..2),
        }
        .map(move |n| t + n)
    };
    result.flat_map_inplace(&func);
    let mut expected = vec.into_iter().flat_map(func).collect::<Vec<_>>();
    assert_eq!(expected.sort(), result.sort());
}
