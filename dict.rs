use core::ptr;
use core::marker::PhantomData;

use free;

pub struct Dict<V> {
    root : *mut V,
}

#[derive(Clone, Copy)]
pub struct DictNode<K, V> {
    pub key : K,
    left : *mut V,
    right : *mut V,
}

impl<K,V> DictNode<K,V> {
    pub fn new(key : K) -> DictNode<K, V> {
        DictNode { key : key, left : null(), right : null() }
    }

    pub fn init(&mut self, key : K) {
        self.key = key;
    }
}

pub trait DictItem {
    type Key;

    fn node<'a>(&'a mut self) -> &'a mut DictNode<Self::Key, Self> where Self: core::marker::Sized;
    // Figure out a way to implement a node->item function, then we can remove
    // "T" from the nodes, do links between nodes instead of items, and use a
    // single copy of the linking code.
}

fn null<T>() -> *mut T { ptr::null_mut() }
fn node<'a, T : DictItem>(p : *mut T) -> &'a mut DictNode<T::Key, T> {
    unsafe { (*p).node() }
}

impl<V : DictItem> Dict<V> where V::Key: Ord + Copy {
    #[allow(dead_code)]
    pub fn empty() -> Dict<V> {
        Dict { root : null() }
    }

    pub fn find<'a>(&mut self, key : V::Key) -> Option<&'a mut V> {
        return self.find_(key);
    }

    // Return the greatest item with key <= key
    #[inline(never)]
    fn find_<'a>(&self, key : V::Key) -> Option<&'a mut V> {
        let mut item = self.root;
        let mut max : *mut V = null();
        while !item.is_null() {
            let ikey : V::Key = node(item).key;
            if ikey <= key {
                if max.is_null() {
                    max = item;
                } else {
                    let maxKey : V::Key = node(max).key;
                    if maxKey < ikey {
                        max = item;
                    }
                }
            }
            item = node(item).right;
        }
        if max.is_null() { None } else { unsafe { Some(&mut *max) } }
    }

    #[inline(always)]
    pub fn find_const<'a>(&self, key : V::Key) -> Option<&'a V> {
        match self.find_(key) {
            Some(x) => Some(&*x),
            None => None
        }
    }

    #[inline(never)]
    pub fn insert<'a>(&mut self, item : *mut V) -> &'a mut V {
        node(item).left = null();
        node(item).right = self.root;
        self.root = item;
        unsafe { &mut *item }
    }

    pub fn remove(&mut self, key: V::Key) {
        let mut p : *mut *mut V = &mut self.root;
        unsafe {
            while !(*p).is_null() {
                let item = *p;
                if node(item).key == key {
                    *p = node(item).right;
                    free(item);
                    break;
                }
                p = &mut node(item).right as *mut*mut V;
            }
        }
    }

    pub fn remove_range_exclusive(&mut self, start: V::Key, end: V::Key) {
        unsafe {
            let mut p : *mut *mut V = &mut self.root;
            while (*p).is_null() {
                let item = *p;
                if start < node(item).key && node(item).key < end {
                    *p = node(item).right;
                    free(item);
                } else {
                    p = &mut node(item).right as *mut*mut V;
                }
            }
        }
    }

    pub fn pop<'a>(&mut self) -> Option<&'a mut V> {
        if self.root.is_null() {
            return None;
        }
        unsafe {
            let res = self.root;
            self.root = node(res).right;
            node(res).right = null();
            return Some(&mut *res);
        }
    }

    pub fn iter<'a>(&'a self) -> DictIter<'a, V> {
        DictIter { p: self.root, phantomdata: PhantomData::<&'a V> }
    }
}

pub struct DictIter<'a, T : 'a> {
    p : *mut T,
    phantomdata : PhantomData<&'a T>
}

impl<'a, V : DictItem> Iterator for DictIter<'a, V> where V::Key: Copy {
    type Item = (V::Key, &'a mut V);

    fn next(&mut self) -> Option<(V::Key, &'a mut V)> {
        if self.p.is_null() {
            None
        } else {
            let res = self.p;
            self.p = node(res).right;
            unsafe { Some((node(res).key, &mut *res)) }
        }
    }
}
