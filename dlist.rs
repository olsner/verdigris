use core::option::*;
use core::iter::*;

pub struct DList<T> {
    head : *mut T,
    tail : *mut T,
}

pub struct DListNode<T> {
    prev : *mut T,
    next : *mut T,
}

impl<T> DListNode<T> {
    pub fn new() -> DListNode<T> {
        DListNode { prev : null(), next : null() }
    }
}

pub trait DListItem {
    fn node<'a>(&'a mut self) -> &'a mut DListNode<Self>;
}

pub fn not_null<T>(p : *mut T) -> bool { p != 0 as *mut T }
pub fn null<T>() -> *mut T { 0 as *mut T }
fn node<'a, T : DListItem>(p : *mut T) -> &'a mut DListNode<T> {
    unsafe { (*p).node() }
}

impl<T : DListItem> DList<T> {
    pub fn empty() -> DList<T> {
        DList { head : null(), tail : null() }
    }

    pub fn append(&mut self, item : *mut T) {
        if not_null(self.tail) {
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

    pub fn remove(&mut self, item : *mut T) -> *mut T {
        let prev = node(item).prev;
        let next = node(item).next;

        node(item).prev = null();
        if not_null(prev) {
            node(prev).next = next;
        }
        node(item).next = null();
        if not_null(next) {
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

    pub fn iter<'a>(&'a self) -> DListIter<'a, T> {
        DListIter { p: self.head }
    }
}

struct DListIter<'a, T> {
    p : *mut T,
}

impl<'a, T : DListItem> Iterator<&'a mut T> for DListIter<'a, T> {
    fn next(&mut self) -> Option<&'a mut T> {
        if not_null(self.p) {
            let res = self.p;
            self.p = node(res).next;
            unsafe { Some(&mut *res) }
        } else {
            None
        }
    }
}
