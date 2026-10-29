//! A map that iterates in the order its keys were added, as JavaScript's Map does: setting a
//! key it has keeps the key's place, and a key deleted and set again goes last. It stands for
//! the Maps of Pele's model, of its resolveUsecase and of its propertyOrder.

use std::collections::HashMap;
use std::hash::Hash;

#[derive(Clone, Debug)]
pub(crate) struct Ordered<K, V> {
    /// The entries in the order they were added, none for one deleted.
    entries: Vec<Option<(K, V)>>,
    index: HashMap<K, usize>,
}

impl<K, V> Default for Ordered<K, V> {
    fn default() -> Self {
        Ordered {
            entries: Vec::new(),
            index: HashMap::new(),
        }
    }
}

impl<K: Clone + Eq + Hash, V> Ordered<K, V> {
    /// The value of a key, as Map's get gives it.
    pub(crate) fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        let &i = self.index.get(key)?;
        self.entries[i].as_ref().map(|(_, v)| v)
    }

    /// The value of a key, to change.
    pub(crate) fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        let &i = self.index.get(key)?;
        self.entries[i].as_mut().map(|(_, v)| v)
    }

    /// Whether the map has a key, as Map's has says.
    pub(crate) fn has<Q>(&self, key: &Q) -> bool
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        self.index.contains_key(key)
    }

    /// Sets the value of a key, which keeps its place if the map has it and goes last if
    /// not, as Map's set does.
    pub(crate) fn set(&mut self, key: K, value: V) {
        if let Some(&i) = self.index.get(&key) {
            if let Some(entry) = self.entries[i].as_mut() {
                entry.1 = value;
            }
            return;
        }
        self.index.insert(key.clone(), self.entries.len());
        self.entries.push(Some((key, value)));
    }

    /// Takes a key out, as Map's delete does.
    pub(crate) fn delete<Q>(&mut self, key: &Q)
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        if let Some(i) = self.index.remove(key) {
            self.entries[i] = None;
        }
    }

    /// The keys and values, in order, as iterating a Map gives them.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.entries.iter().flatten().map(|(k, v)| (k, v))
    }

    /// The values, in order.
    pub(crate) fn values(&self) -> impl Iterator<Item = &V> {
        self.iter().map(|(_, v)| v)
    }

    /// The keys, in order.
    pub(crate) fn keys(&self) -> impl Iterator<Item = &K> {
        self.iter().map(|(k, _)| k)
    }

    /// The values, in order, as a list.
    pub(crate) fn into_values(self) -> Vec<V> {
        self.entries.into_iter().flatten().map(|(_, v)| v).collect()
    }
}
