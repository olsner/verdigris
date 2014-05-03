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

pub fn not_null<U, T : RawPtr<U>>(p : T) -> bool { p.is_not_null() }
fn null<T>() -> *mut T { RawPtr::null() }
fn node<'a, K, T : DictItem<K>>(p : *mut T) -> &'a mut DictNode<K, T> {
    unsafe { (*p).node() }
}

impl<K : Ord + Copy, V : DictItem<K>> Dict<V> {
    pub fn empty() -> Dict<V> {
        Dict { root : null() }
    }

    // Return the greatest item with key <= key
    pub fn find<'a>(&'a mut self, key : K) -> Option<&'a mut V> {
        let mut item = self.root;
        let mut max = null();
        while item.is_not_null() {
            let ikey : K = node(item).key;
            let maxKey : K = node(max).key;
            if ikey <= key && (max.is_null() || maxKey < ikey) {
                max = item;
            }
            item = node(item).right;
        }
        if max.is_null() { None } else { unsafe { Some(&mut *max) } }
    }

    pub fn insert(&mut self, item : *mut V) {
        node(item).left = null();
        node(item).right = self.root;
        self.root = item;
    }
}