//! Hints about holes in repeated buffer uploads. A hint selects the read
//! boundaries only: the kernel checks each read and each hole is queried anew.

use std::cell::RefCell;

type Span = (usize, usize);

#[derive(Default)]
struct Plan {
    start: usize,
    holes: Vec<Span>,
}

#[derive(Default)]
struct Plans {
    slots: Vec<Plan>,
    scratch: Vec<Span>,
}

thread_local! { static PLANS: RefCell<Plans> = RefCell::default(); }

/// Upload to a live host buffer. Large shadow ranges often cover a few
/// holes among hundreds of individually mapped, adjacent physical views.
/// Once we know a likely hole, try all readable views before it in one read.
pub(super) fn copy(dst: *mut u8, src: *const u8, len: usize) {
    const SLOTS: usize = 2048;
    const MAX_HOLES: usize = 64;
    let start = src as usize;
    let end = start + len;
    PLANS.with(|plans| {
        let Ok(mut plans) = plans.try_borrow_mut() else {
            // A reentrant callback can still copy without using the hints.
            super::copy_mapped_checked(dst, src, len, false);
            return;
        };
        let Plans { slots, scratch } = &mut *plans;
        let key = ((start as u64).wrapping_mul(0x9e3779b97f4a7c15) >> 53) as usize;
        let hints = slots
            .get(key)
            .filter(|p| p.start == start && p.holes.first().is_some_and(|&(lo, _)| lo < end));
        if hints.is_none() && super::first_read(src, dst, len) {
            return;
        }
        #[cfg(feature = "diagnostics")]
        crate::copystat::add(crate::copystat::C::Fallbacks, 1);

        scratch.clear();
        let mut remember = |lo: usize, hi: usize| {
            if let Some((_, last)) = scratch.last_mut()
                && *last == lo
            {
                *last = hi;
            } else if scratch.len() < MAX_HOLES {
                scratch.push((lo, hi));
            } else if scratch.len() == MAX_HOLES {
                super::refresh_timing::capped();
            }
        };
        let mut walk = |lo: usize, hi: usize, query_first: bool| {
            if lo == hi {
                return;
            }
            let out = dst.wrapping_add(lo - start);
            if query_first || !super::first_read(lo as *const u8, out, hi - lo) {
                super::copy_regions(out, lo, hi, false, &mut remember);
            }
        };
        let mut at = start;
        if let Some(plan) = hints {
            for &(lo, hi) in &plan.holes {
                if lo >= end {
                    break;
                }
                walk(at, lo, false);
                at = hi.min(end);
                walk(lo, at, true);
            }
        }
        walk(at, end, hints.is_none());

        if slots.is_empty() {
            slots.resize_with(SLOTS, Plan::default);
        }
        let slot = &mut slots[key];
        slot.start = start;
        std::mem::swap(&mut slot.holes, scratch);
    });
}

#[cfg(test)]
mod tests {
    use super::copy;
    use windows_sys::Win32::System::Memory::{
        MEM_COMMIT, MEM_DECOMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE, VirtualAlloc,
        VirtualFree,
    };

    #[test]
    fn changing_holes_and_lengths_are_visible_on_the_next_copy() {
        const B: usize = 64 << 10;
        // SAFETY: a fresh allocation owned by this test.
        unsafe {
            let base = VirtualAlloc(
                std::ptr::null(),
                7 * B,
                MEM_RESERVE | MEM_COMMIT,
                PAGE_READWRITE,
            )
            .cast::<u8>();
            assert!(!base.is_null());
            let mut expected = vec![71; 7 * B];
            base.write_bytes(71, expected.len());
            for i in [1, 3, 5] {
                assert_ne!(VirtualFree(base.add(i * B).cast(), B, MEM_DECOMMIT), 0);
                expected[i * B..(i + 1) * B].fill(0);
            }
            let mut out = vec![0xee; expected.len()];
            copy(out.as_mut_ptr(), base, out.len());
            assert_eq!(out, expected);

            // The old middle hole becomes readable; a former readable span
            // gains a hole. The next request ends part-way through a known hole.
            let mid = base.add(3 * B);
            assert_eq!(
                VirtualAlloc(mid.cast(), B, MEM_COMMIT, PAGE_READWRITE).cast::<u8>(),
                mid
            );
            mid.write_bytes(93, B);
            expected[3 * B..4 * B].fill(93);
            assert_ne!(VirtualFree(base.add(4 * B).cast(), B, MEM_DECOMMIT), 0);
            expected[4 * B..5 * B].fill(0);
            for bytes in [5 * B + 17, 7 * B, 2 * B, 7 * B] {
                out.fill(0xee);
                copy(out.as_mut_ptr(), base, bytes);
                assert_eq!(out[..bytes], expected[..bytes]);
                assert!(out[bytes..].iter().all(|&x| x == 0xee));
            }
            assert_ne!(VirtualFree(base.cast(), 0, MEM_RELEASE), 0);
        }
    }
}
