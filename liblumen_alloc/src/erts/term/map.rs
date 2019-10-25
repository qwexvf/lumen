use core::alloc::Layout;
use core::cmp;
use core::convert::TryFrom;
use core::fmt::{self, Debug, Display};
use core::hash::{Hash, Hasher};
use core::mem;
use core::ptr;

use alloc::vec::Vec;

use hashbrown::HashMap;

use crate::erts::exception::system::Alloc;
use crate::erts::process::HeapAlloc;

use super::prelude::*;

#[derive(Clone)]
#[repr(C)]
pub struct Map {
    header: Header<Map>,
    value: HashMap<Term, Term>,
}

impl Map {
    pub(in crate::erts) fn from_hash_map(value: HashMap<Term, Term>) -> Self {
        Self {
            header: Header::from_map(&value),
            value
        }
    }

    pub(in crate::erts) fn from_slice(slice: &[(Term, Term)]) -> Self {
        let mut value: HashMap<Term, Term> = HashMap::with_capacity(slice.len());

        for (entry_key, entry_value) in slice {
            value.insert(*entry_key, *entry_value);
        }

        Self::from_hash_map(value)
    }

    pub fn from_list(list: Term) -> Option<HashMap<Term, Term>> {
        match list.decode().unwrap() {
            TypedTerm::Nil => Some(HashMap::new()),
            TypedTerm::List(cons_ptr) => {
                let cons = cons_ptr.as_ref();
                let mut map = HashMap::new();

                for term_result in cons.into_iter() {
                    if let Ok(term) = term_result {
                        if let Ok(TypedTerm::Tuple(tuple)) = term.decode() {
                            if tuple.len() == 2 {
                                map.insert(tuple[0], tuple[1]);
                                continue;
                            }
                        } else {
                            continue;
                        }
                    }

                    return None;
                }

                Some(map)
            }
            _ => None,
        }
    }

    pub fn get(&self, key: Term) -> Option<Term> {
        self.value.get(&key).copied()
    }

    pub fn take(&self, key: Term) -> Option<(Term, HashMap<Term, Term>)> {
        if self.is_key(key) {
            let mut map = self.value.clone();
            let value = map.remove(&key).unwrap();

            Some((value, map))
        } else {
            None
        }
    }

    pub fn is_key(&self, key: Term) -> bool {
        self.value.contains_key(&key)
    }

    pub fn keys(&self) -> Vec<Term> {
        self.value.keys().into_iter().copied().collect()
    }

    pub fn values(&self) -> Vec<Term> {
        self.value.values().into_iter().copied().collect()
    }

    pub fn len(&self) -> usize {
        self.value.len()
    }

    pub fn remove(&self, key: Term) -> Option<HashMap<Term, Term>> {
        if self.is_key(key) {
            let mut map = self.value.clone();
            map.remove(&key);
            Some(map)
        } else {
            None
        }
    }

    pub fn update(&self, key: Term, value: Term) -> Option<HashMap<Term, Term>> {
        if self.is_key(key) {
            let mut map = self.value.clone();
            map.insert(key, value);
            Some(map)
        } else {
            None
        }
    }

    pub fn put(&self, key: Term, value: Term) -> Option<HashMap<Term, Term>> {
        if self.get(key).map_or(false, |val| val == value) {
            None
        } else {
            let mut map = self.value.clone();
            map.insert(key, value);
            Some(map)
        }
    }

    // Private

    fn sorted_keys(&self) -> Vec<Term> {
        let mut key_vec: Vec<Term> = Vec::new();
        key_vec.extend(self.value.keys());
        key_vec.sort_unstable_by(|key1, key2| key1.cmp(&key2));

        key_vec
    }
}

impl AsRef<HashMap<Term, Term>> for Boxed<Map> {
    fn as_ref(&self) -> &HashMap<Term, Term> {
        &self.as_ref().value
    }
}

impl AsRef<HashMap<Term, Term>> for Map {
    fn as_ref(&self) -> &HashMap<Term, Term> {
        &self.value
    }
}

impl crate::borrow::CloneToProcess for Map {
    fn clone_to_heap<A>(&self, heap: &mut A) -> Result<Term, Alloc>
    where
        A: ?Sized + HeapAlloc,
    {
        let layout = Layout::for_value(self);
        let ptr = unsafe { heap.alloc_layout(layout)?.as_ptr() };

        let self_value = &self.value;
        let mut heap_value = HashMap::with_capacity(self_value.len());

        for (entry_key, entry_value) in self_value {
            let heap_entry_key = entry_key.clone_to_heap(heap)?;
            let heap_entry_value = entry_value.clone_to_heap(heap)?;
            heap_value.insert(heap_entry_key, heap_entry_value);
        }

        // Clone to ensure `value` remains valid if caller is dropped
        let heap_self = Self {
            header: self.header.clone(),
            value: heap_value,
        };

        let size = mem::size_of_val(self);
        unsafe {
            ptr::copy_nonoverlapping(&heap_self as *const _ as *const u8, ptr as *mut u8, size);
        }

        mem::forget(heap_self);

        Ok((ptr as *mut Self).into())
    }
}

impl Debug for Map {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Map")
            .field("header", &self.header)
            .field("value", &self.value)
            .finish()
    }
}

impl Display for Map {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.value)
    }
}

impl Eq for Map {}

impl Hash for Map {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for key in self.sorted_keys() {
            let value = self.value[&key];

            key.hash(state);
            value.hash(state);
        }
    }
}

impl PartialEq for Map {
    fn eq(&self, other: &Map) -> bool {
        self.value.eq(&other.value)
    }
}
impl<T> PartialEq<Boxed<T>> for Map
where
    T: PartialEq<Map>,
{
    #[inline]
    fn eq(&self, other: &Boxed<T>) -> bool {
        other.as_ref().eq(self)
    }
}

impl PartialOrd for Map {
    fn partial_cmp(&self, other: &Map) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<T> PartialOrd<Boxed<T>> for Map
where
    T: PartialOrd<Map>,
{
    #[inline]
    fn partial_cmp(&self, other: &Boxed<T>) -> Option<cmp::Ordering> {
        other.as_ref().partial_cmp(self).map(|o| o.reverse())
    }
}

impl Ord for Map {
    /// > * Maps are compared by size, then by keys in ascending term order,
    /// >   then by values in key order.   In the specific case of maps' key
    /// >   ordering, integers are always considered to be less than floats.
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        match self.len().cmp(&other.len()) {
            cmp::Ordering::Equal => {
                let self_key_vec = self.sorted_keys();
                let other_key_vec = other.sorted_keys();

                match self_key_vec.cmp(&other_key_vec) {
                    cmp::Ordering::Equal => {
                        let self_value = &self.value;
                        let other_value = &other.value;
                        let mut final_ordering = cmp::Ordering::Equal;

                        for key in self_key_vec {
                            match self_value
                                .get(&key)
                                .unwrap()
                                .cmp(other_value.get(&key).unwrap())
                            {
                                cmp::Ordering::Equal => continue,
                                ordering => {
                                    final_ordering = ordering;

                                    break;
                                }
                            }
                        }

                        final_ordering
                    }
                    ordering => ordering,
                }
            }
            ordering => ordering,
        }
    }
}

impl TryFrom<TypedTerm> for Boxed<Map> {
    type Error = TypeError;

    fn try_from(typed_term: TypedTerm) -> Result<Boxed<Map>, Self::Error> {
        match typed_term {
            TypedTerm::Map(map) => Ok(map),
            _ => Err(TypeError),
        }
    }
}
