//! Process management syscalls
use crate::{
    config::{MAX_SYSCALL_NUM, PAGE_SIZE},
    mm::VirtAddr,
    task::{
        change_program_brk, exit_current_and_run_next, push_unnamed_area,
        suspend_current_and_run_next, TaskStatus,
    },
    timer::get_time_us,
};

use crate::mm::translate_va_2_pa;
use crate::task::get_cur_run_time_ms;
use crate::task::get_syscall_times;
use crate::task::get_task_status;
use crate::task::update_time;

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// Task information
#[allow(dead_code)]
pub struct TaskInfo {
    /// Task status in it's life cycle
    status: TaskStatus,
    /// The numbers of syscall called by task
    syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of task
    time: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
// FIXME: not solve splitted by two pages
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    let va: VirtAddr = VirtAddr::from(_ts as usize);
    let pa = translate_va_2_pa(va).unwrap();
    let ts = pa.0 as usize as *mut TimeVal;
    let time = get_time_us();
    unsafe {
        *ts = TimeVal {
            sec: time / 1_000_000,
            usec: time % 1_000_000,
        };
    };
    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
// FIXME: not solve splitted by two pages
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    update_time();
    let va: VirtAddr = VirtAddr::from(_ti as usize);
    let pa = translate_va_2_pa(va).unwrap();
    let ti = pa.0 as usize as *mut TaskInfo;
    unsafe {
        *ti = TaskInfo {
            status: get_task_status(),
            syscall_times: get_syscall_times(),
            time: get_cur_run_time_ms(),
        };
    };
    0
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, port: usize) -> isize {
    debug!("start: {:#x}, len: {:#x}, port: {:#x}", start, len, port);
    if (port & (!0x7) != 0) || (port & 0x7 == 0) || (start % PAGE_SIZE != 0) {
        return -1;
    }
    let flags: u8 = (((port << 1) & 0b0110) | 0b10000) as u8;
    if len == 0 {
        return -1;
    }
    debug!("sys_map start: {:#x}, end: {:#x}", start, start + len);
    if !push_unnamed_area(start, start + len, flags) {
        return -1;
    }
    0
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!("kernel: sys_munmap NOT IMPLEMENTED YET!");
    -1
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
