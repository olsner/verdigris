use core::prelude::*;
use core::ptr;
use core::marker::PhantomData;

use util::abort;

pub struct DList<T> {
    head : *mut T,
    tail : *mut T,
}

pub struct DListNode<T> {
    prev : *mut T,
    next : *mut T,
}

impl<T> DListNode<T> {
    // Just silence the warning. Currently all users rely on init-to-0 to
    // initialize their DListNodes
    #[allow(dead_code)]
    pub fn new() -> DListNode<T> {
        DListNode { prev : null(), next : null() }
    }
}

pub trait DListItem {
    fn node<'a>(&'a mut self) -> &'a mut DListNode<Self>;
    // Figure out a way to implement a node->item function, then we can remove
    // "T" from the nodes, do links between nodes instead of items, and use a
    // single copy of the linking code.
}

fn null<T>() -> *mut T { ptr::null_mut() }
fn node<'a, T : DListItem>(p : *mut T) -> &'a mut DListNode<T> {
    unsafe { (*p).node() }
}

impl<T : DListItem> DList<T> {
    pub fn empty() -> DList<T> {
        DList { head : null(), tail : null() }
    }

    #[inline(never)]
    pub fn append(&mut self, item : *mut T) {
        if !(node(item).prev.is_null() && node(item).next.is_null()) {
            abort("appending item already in list");
        }
        if !self.tail.is_null() {
            let tail = self.tail;
            self.tail = item;
            node(tail).next = item;
            node(item).prev = tail;
        } else {
            self.tail = item;
            self.head = item;
        }
    }

    pub fn pop(&mut self) -> Option<*mut T> {
        let head = self.head;
        if head != 0 as *mut T {
            return Some(self.remove(head));
        } else {
            return None;
        }
    }

    #[inline(never)]
    pub fn remove(&mut self, item : *mut T) -> *mut T {
        let prev = node(item).prev;
        let next = node(item).next;

        node(item).prev = null();
        if !prev.is_null() {
            node(prev).next = next;
        }
        node(item).next = null();
        if !next.is_null() {
            node(next).prev = prev;
        }

        if self.head == item {
            self.head = next;
        }
        if self.tail == item {
            self.tail = prev;
        }

        return item;
    }

    // FIXME Hack: returns an iterator unconnected to the collection's lifetime,
    // so that it's possible to remove entries while iterating.
    pub fn iter<'a>(&self) -> DListIter<'a, T> {
        DListIter { p: self.head, phantomdata: PhantomData::<&'a T> }
    }
}

struct DListIter<'a, T : 'a> {
    p : *mut T,
    phantomdata : PhantomData<&'a T>
}

impl<'a, T : DListItem> Iterator for DListIter<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<&'a mut T> {
        if self.p.is_null() {
            None
        } else {
            let res = self.p;
            self.p = node(res).next;
            unsafe { Some(&mut *res) }
        }
    }
}
