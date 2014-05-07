use core::prelude::*;

pub struct Dict<T> {
    root : *mut T,
}

pub struct DictNode<K, V> {
    pub key : K,
    left : *mut V,
    right : *mut V,
}

impl<K,V> DictNode<K,V> {
    pub fn new(key : K) -> DictNode<K, V> {
        DictNode { key : key, left : null(), right : null() }
    }
}

pub trait DictItem<K> {
    fn node<'a>(&'a mut self) -> &'a mut DictNode<K, Self>;
    // Figure out a way to implement a node->item function, then we can remove
    // "T" from the nodes, do links between nodes instead of items, and use a
    // single copy of the linking code.
}

fn null<T>() -> *mut T { RawPtr::null() }
fn node<'a, K, T : DictItem<K>>(p : *mut T) -> &'a mut DictNode<K, T> {
    unsafe { (*p).node() }
}

impl<K : Ord + Copy, V : DictItem<K>> Dict<V> {
    pub fn empty() -> Dict<V> {
        Dict { root : null() }
    }

    // Return the greatest item with key <= key
    #[inline(never)]
    pub fn find<'a>(&mut self, key : K) -> Option<&'a mut V> {
        let mut item = self.root;
        let mut max = null();
        while item.is_not_null() {
            let ikey : K = node(item).key;
            if ikey <= key {
                if max.is_null() {
                    max = item;
                } else {
                    let maxKey : K = node(max).key;
                    if maxKey < ikey {
                        max = item;
                    }
                }
            }
            item = node(item).right;
        }
        if max.is_null() { None } else { unsafe { Some(&mut *max) } }
    }

    #[inline(never)]
    pub fn insert<'a>(&mut self, item : *mut V) -> &'a mut V {
        node(item).left = null();
        node(item).right = self.root;
        self.root = item;
        unsafe { &mut *item }
    }

    pub fn iter<'a>(&'a self) -> DictIter<'a, V> {
        DictIter { p: self.root }
    }
}

struct DictIter<'a, T> {
    p : *mut T,
}

impl<'a, K : Copy, V : DictItem<K>> Iterator<(K, &'a mut V)> for DictIter<'a, V> {
    fn next(&mut self) -> Option<(K, &'a mut V)> {
        if self.p.is_not_null() {
            let res = self.p;
            self.p = node(res).right;
            unsafe { Some((node(res).key, &mut *res)) }
        } else {
            None
        }
    }
}
