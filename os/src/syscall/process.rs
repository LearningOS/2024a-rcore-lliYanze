//! Process management syscalls
//!
use alloc::sync::Arc;

use crate::{
    config::{MAX_SYSCALL_NUM, PAGE_SIZE},
    fs::{open_file, OpenFlags},
    mm::{translated_refmut, translated_str, VirtAddr},
    task::{
        add_task, current_task, current_user_token, exit_current_and_run_next, push_unnamed_area,
        suspend_current_and_run_next, TaskStatus,
    },
    timer::get_time_us,
};

use crate::mm::translate_va_2_pa;
use crate::task::get_cur_run_time_ms;
use crate::task::get_syscall_times;
use crate::task::get_task_status;
use crate::task::make_task_controlbrock;
use crate::task::remove_unnamed_area;
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

pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

pub fn sys_yield() -> isize {
    //trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}

pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let task = current_task().unwrap();
        task.exec(all_data.as_slice());
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    //trace!("kernel: sys_waitpid");
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
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
pub fn sys_munmap(start: usize, len: usize) -> isize {
    if start % PAGE_SIZE != 0 {
        return -1;
    }
    if len == 0 {
        return -1;
    }
    if remove_unnamed_area(start, start + len) {
        return 0;
    }
    -1
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(path: *const u8) -> isize {
    let current_task = current_task().unwrap();
    let mut parent_inner = current_task.inner_exclusive_access();
    trace!("kernel:pid[{}] sys_spawn", current_task.pid.0);
    let token = parent_inner.memory_set.token();
    let path = translated_str(token, path);
    if let Some(new_task) = make_task_controlbrock(path.as_str()) {
        let new_pid = new_task.pid.0;
        // parent get new child pid
        parent_inner.children.push(new_task.clone());
        debug!("sys spawn new pid: {}", new_pid);
        add_task(new_task);
        new_pid as isize
    } else {
        -1
    }
}

// YOUR JOB: Set task priority.
pub fn sys_set_priority(_prio: isize) -> isize {
    trace!(
        "kernel:pid[{}] sys_set_priority NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    -1
}
