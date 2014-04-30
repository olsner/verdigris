use core::option::*;

pub struct DList<T> {
    head : *mut T,
    tail : *mut T,
}

pub struct DListNode<T> {
    prev : *mut T,
    next : *mut T,
}

pub trait DListItem {
    fn node<'a>(&'a mut self) -> &'a mut DListNode<Self>;
}

fn not_null<T>(p : *mut T) -> bool { p != 0 as *mut T }
fn null<T>() -> *mut T { 0 as *mut T }
fn node<'a, T : DListItem>(p : *mut T) -> &'a mut DListNode<T> {
    unsafe { (*p).node() }
}

impl<T : DListItem> DList<T> {
    pub fn append(&mut self, item : *mut T) {
        if self.tail == 0 as *mut T {
            self.tail = item;
            self.head = item;
        } else {
            let tail = self.tail;
            self.tail = item;
            node(tail).next = item;
            node(item).prev = tail;
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
}
