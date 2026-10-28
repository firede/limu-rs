//! Items kept as JavaScript's Map keeps them: in the order they were first added.

use std::collections::HashMap;

/// Items kept in the order they were added, and found by their ids, as a JavaScript Map
/// keeps them.
#[derive(Clone, Debug)]
pub(crate) struct Ordered<T> {
    items: Vec<T>,
    at: HashMap<String, usize>,
}

impl<T> Default for Ordered<T> {
    fn default() -> Self {
        Ordered {
            items: Vec::new(),
            at: HashMap::new(),
        }
    }
}

impl<T> Ordered<T> {
    pub(crate) fn get(&self, id: &str) -> Option<&T> {
        self.at.get(id).map(|&i| &self.items[i])
    }

    pub(crate) fn get_mut(&mut self, id: &str) -> Option<&mut T> {
        self.at.get(id).map(|&i| &mut self.items[i])
    }

    pub(crate) fn contains(&self, id: &str) -> bool {
        self.at.contains_key(id)
    }

    /// Sets the item of an id, in its place where it has one and last where it does not, and
    /// gives its place.
    pub(crate) fn set(&mut self, id: &str, item: T) -> usize {
        match self.at.get(id) {
            Some(&i) => {
                self.items[i] = item;
                i
            }
            None => {
                let i = self.items.len();
                self.at.insert(id.to_string(), i);
                self.items.push(item);
                i
            }
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.items.len()
    }

    /// The place of the item of an id, in the order the items were added.
    pub(crate) fn index_of(&self, id: &str) -> Option<usize> {
        self.at.get(id).copied()
    }

    /// The item at a place [`index_of`](Ordered::index_of) gave.
    pub(crate) fn at_mut(&mut self, i: usize) -> &mut T {
        &mut self.items[i]
    }

    pub(crate) fn values(&self) -> std::slice::Iter<'_, T> {
        self.items.iter()
    }

    pub(crate) fn values_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.items.iter_mut()
    }
}

impl<T> std::ops::Index<usize> for Ordered<T> {
    type Output = T;

    fn index(&self, i: usize) -> &T {
        &self.items[i]
    }
}

impl<T> std::ops::IndexMut<usize> for Ordered<T> {
    fn index_mut(&mut self, i: usize) -> &mut T {
        &mut self.items[i]
    }
}
