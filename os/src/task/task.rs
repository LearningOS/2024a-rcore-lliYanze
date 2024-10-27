//! Types related to task management

use super::TaskContext;
use crate::config::MAX_SYSCALL_NUM;

/// The task control block (TCB) of a task.
#[derive(Copy, Clone)]
pub struct TaskControlBlock {
    /// The task status in it's lifecycle
    pub task_status: TaskStatus,
    /// The task context
    pub task_cx: TaskContext,
    /// The number of syscalls called by the task
    pub syscall_times: [u32; MAX_SYSCALL_NUM],
    /// The total running time of the task
    pub time: usize,
    /// the start time of the task
    pub start_time: usize,
}

impl TaskControlBlock {
    /// Create a new TaskControlBlock
    pub fn new() -> Self {
        Self {
            task_status: TaskStatus::UnInit,
            task_cx: TaskContext::zero_init(),
            syscall_times: [0; MAX_SYSCALL_NUM],
            time: 0,
            start_time: 0,
        }
    }

    /// update the start time of the task
    pub fn update_syscall_times(&mut self, syscall_id: usize) {
        self.syscall_times[syscall_id] += 1;
    }
}

/// The status of a task
#[derive(Copy, Clone, PartialEq)]
pub enum TaskStatus {
    /// uninitialized
    UnInit,
    /// ready to run
    Ready,
    /// running
    Running,
    /// exited
    Exited,
}
