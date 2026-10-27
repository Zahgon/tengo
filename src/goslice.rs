//! Go slices: a shared backing array plus an (offset, length, capacity) header.
//!
//! Tengo arrays and bytes are Go slices, and the original's `+`, `append`, `splice` and
//! slicing operate on them with Go's semantics: appending within capacity writes into the
//! shared backing array (so two results of `a + x` can alias), and growth follows the Go 1.25
//! runtime (`nextslicecap` then `roundupsize` to a malloc size class).

use std::cell::RefCell;
use std::rc::Rc;

const SIZE_CLASS_TO_SIZE: [u32; 68] = [
    0, 8, 16, 24, 32, 48, 64, 80, 96, 112, 128, 144, 160, 176, 192, 208, 224, 240, 256, 288, 320, 352, 384, 416,
    448, 480, 512, 576, 640, 704, 768, 896, 1024, 1152, 1280, 1408, 1536, 1792, 2048, 2304, 2688, 3072, 3200, 3456,
    4096, 4864, 5376, 6144, 6528, 6784, 6912, 8192, 9472, 9728, 10240, 10880, 12288, 13568, 14336, 16384, 18432,
    19072, 20480, 21760, 24576, 27264, 28672, 32768,
];

const SIZE_TO_CLASS8: [u8; 129] = [
    0, 1, 2, 3, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13, 14, 14, 15, 15, 16, 16, 17, 17, 18,
    18, 19, 19, 19, 19, 20, 20, 20, 20, 21, 21, 21, 21, 22, 22, 22, 22, 23, 23, 23, 23, 24, 24, 24, 24, 25, 25, 25,
    25, 26, 26, 26, 26, 27, 27, 27, 27, 27, 27, 27, 27, 28, 28, 28, 28, 28, 28, 28, 28, 29, 29, 29, 29, 29, 29, 29,
    29, 30, 30, 30, 30, 30, 30, 30, 30, 31, 31, 31, 31, 31, 31, 31, 31, 31, 31, 31, 31, 31, 31, 31, 31, 32, 32, 32,
    32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32,
];

const SIZE_TO_CLASS128: [u8; 249] = [
    32, 33, 34, 35, 36, 37, 37, 38, 38, 39, 39, 40, 40, 40, 41, 41, 41, 42, 43, 43, 44, 44, 44, 44, 44, 45, 45, 45,
    45, 45, 45, 46, 46, 46, 46, 47, 47, 47, 47, 47, 47, 48, 48, 48, 49, 49, 50, 51, 51, 51, 51, 51, 51, 51, 51, 51,
    51, 52, 52, 52, 52, 52, 52, 52, 52, 52, 52, 53, 53, 54, 54, 54, 54, 55, 55, 55, 55, 55, 56, 56, 56, 56, 56, 56,
    56, 56, 56, 56, 56, 57, 57, 57, 57, 57, 57, 57, 57, 57, 57, 58, 58, 58, 58, 58, 58, 59, 59, 59, 59, 59, 59, 59,
    59, 59, 59, 59, 59, 59, 59, 59, 59, 60, 60, 60, 60, 60, 60, 60, 60, 60, 60, 60, 60, 60, 60, 60, 60, 61, 61, 61,
    61, 61, 62, 62, 62, 62, 62, 62, 62, 62, 62, 62, 62, 63, 63, 63, 63, 63, 63, 63, 63, 63, 63, 64, 64, 64, 64, 64,
    64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 64, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65,
    65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 66, 66, 66, 66, 66, 66, 66, 66, 66, 66, 66, 67, 67, 67, 67, 67, 67, 67,
    67, 67, 67, 67, 67, 67, 67, 67, 67, 67, 67, 67, 67, 67, 67, 67, 67, 67, 67, 67, 67, 67, 67, 67, 67,
];

const MAX_SMALL_SIZE: usize = 32768;
const SMALL_SIZE_DIV: usize = 8;
const SMALL_SIZE_MAX: usize = 1024;
const LARGE_SIZE_DIV: usize = 128;
const PAGE_SIZE: usize = 8192;
const MALLOC_HEADER_SIZE: usize = 8;
const MIN_SIZE_FOR_MALLOC_HEADER: usize = 512;

fn div_round_up(n: usize, a: usize) -> usize {
    (n + a - 1) / a
}

/// runtime.roundupsize: the size class an allocation of `size` bytes lands in.
pub fn roundupsize(size: usize, noscan: bool) -> usize {
    let mut req = size;
    if req <= MAX_SMALL_SIZE - MALLOC_HEADER_SIZE {
        if !noscan && req > MIN_SIZE_FOR_MALLOC_HEADER {
            req += MALLOC_HEADER_SIZE;
        }
        if req <= SMALL_SIZE_MAX - 8 {
            return SIZE_CLASS_TO_SIZE[SIZE_TO_CLASS8[div_round_up(req, SMALL_SIZE_DIV)] as usize] as usize - (req - size);
        }
        return SIZE_CLASS_TO_SIZE[SIZE_TO_CLASS128[div_round_up(req - SMALL_SIZE_MAX, LARGE_SIZE_DIV)] as usize] as usize
            - (req - size);
    }
    req += PAGE_SIZE - 1;
    if req < size {
        return size;
    }
    req & !(PAGE_SIZE - 1)
}

/// runtime.nextslicecap.
pub fn nextslicecap(new_len: usize, old_cap: usize) -> usize {
    let mut newcap = old_cap as i64;
    let doublecap = newcap + newcap;
    if new_len as i64 > doublecap {
        return new_len;
    }
    const THRESHOLD: i64 = 256;
    if (old_cap as i64) < THRESHOLD {
        return doublecap as usize;
    }
    loop {
        newcap += (newcap + 3 * THRESHOLD) >> 2;
        if newcap as u64 >= new_len as u64 {
            break;
        }
    }
    if newcap <= 0 {
        return new_len;
    }
    newcap as usize
}

/// The capacity growslice picks for `new_len` elements of `elem_size` bytes.
pub fn grow_cap(new_len: usize, old_cap: usize, elem_size: usize, noscan: bool) -> usize {
    let newcap = nextslicecap(new_len, old_cap);
    let capmem = roundupsize(newcap * elem_size, noscan);
    capmem / elem_size
}

/// A value that can live in a Go slice: its element size and whether it holds pointers.
pub trait SliceElem: Clone {
    const SIZE: usize;
    const NOSCAN: bool;
    fn zero() -> Self;
}

impl SliceElem for u8 {
    const SIZE: usize = 1;
    const NOSCAN: bool = true;
    fn zero() -> Self {
        0
    }
}

/// A Go slice header over a shared backing array. The backing vector always has at least
/// `off + cap` elements.
#[derive(Clone)]
pub struct GoSlice<T: SliceElem> {
    pub buf: Rc<RefCell<Vec<T>>>,
    pub off: usize,
    pub len: usize,
    pub cap: usize,
}

impl<T: SliceElem> Default for GoSlice<T> {
    fn default() -> Self {
        GoSlice::nil()
    }
}

impl<T: SliceElem> GoSlice<T> {
    /// A nil slice (len 0, cap 0).
    pub fn nil() -> Self {
        GoSlice { buf: Rc::new(RefCell::new(Vec::new())), off: 0, len: 0, cap: 0 }
    }

    /// make([]T, len) (and make([]T, len, cap)).
    pub fn make(len: usize, cap: usize) -> Self {
        let cap = cap.max(len);
        GoSlice { buf: Rc::new(RefCell::new(vec![T::zero(); cap])), off: 0, len, cap }
    }

    /// A slice of exactly these elements with len == cap.
    pub fn from_vec_exact(v: Vec<T>) -> Self {
        let n = v.len();
        GoSlice { buf: Rc::new(RefCell::new(v)), off: 0, len: n, cap: n }
    }

    /// A slice of these elements with an explicit capacity (cap >= len).
    pub fn from_vec_cap(mut v: Vec<T>, cap: usize) -> Self {
        let n = v.len();
        let cap = cap.max(n);
        v.resize(cap, T::zero());
        GoSlice { buf: Rc::new(RefCell::new(v)), off: 0, len: n, cap }
    }

    /// Builds a slice the way repeated `s = append(s, x)` from nil would.
    pub fn from_appends(items: Vec<T>) -> Self {
        let mut s = GoSlice::nil();
        for it in items {
            s = s.append(&[it]);
        }
        s
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn get(&self, i: usize) -> T {
        self.buf.borrow()[self.off + i].clone()
    }

    pub fn set(&self, i: usize, v: T) {
        self.buf.borrow_mut()[self.off + i] = v;
    }

    pub fn to_vec(&self) -> Vec<T> {
        self.buf.borrow()[self.off..self.off + self.len].to_vec()
    }

    /// s[lo:hi]
    pub fn slice(&self, lo: usize, hi: usize) -> Self {
        GoSlice { buf: self.buf.clone(), off: self.off + lo, len: hi - lo, cap: self.cap - lo }
    }

    /// append(s, items...)
    pub fn append(&self, items: &[T]) -> Self {
        let n = items.len();
        if n == 0 {
            return self.clone();
        }
        let new_len = self.len + n;
        if new_len <= self.cap {
            {
                let mut b = self.buf.borrow_mut();
                for (k, it) in items.iter().enumerate() {
                    b[self.off + self.len + k] = it.clone();
                }
            }
            return GoSlice { buf: self.buf.clone(), off: self.off, len: new_len, cap: self.cap };
        }
        let newcap = grow_cap(new_len, self.cap, T::SIZE, T::NOSCAN);
        let mut v: Vec<T> = Vec::with_capacity(newcap);
        {
            let b = self.buf.borrow();
            v.extend_from_slice(&b[self.off..self.off + self.len]);
        }
        v.extend_from_slice(items);
        v.resize(newcap, T::zero());
        GoSlice { buf: Rc::new(RefCell::new(v)), off: 0, len: new_len, cap: newcap }
    }

    /// append(s, other...) where other is another slice (read before writing, as Go's
    /// memmove does).
    pub fn append_slice(&self, other: &GoSlice<T>) -> Self {
        let items = other.to_vec();
        self.append(&items)
    }

    pub fn same_backing(&self, other: &GoSlice<T>) -> bool {
        Rc::ptr_eq(&self.buf, &other.buf)
    }
}

/// []byte(s): the conversion allocates roundupsize(len) bytes, so cap can exceed len.
pub fn bytes_from_string(s: &[u8]) -> GoSlice<u8> {
    if s.is_empty() {
        // A zero-length conversion yields a non-nil empty slice with cap 0.
        return GoSlice::from_vec_exact(Vec::new());
    }
    let cap = roundupsize(s.len(), true);
    GoSlice::from_vec_cap(s.to_vec(), cap)
}
