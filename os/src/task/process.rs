//! Implementation of  [`ProcessControlBlock`]

use super::id::RecycleAllocator;
use super::manager::insert_into_pid2process;
use super::TaskControlBlock;
use super::{add_task, SignalFlags};
use super::{pid_alloc, PidHandle};
use crate::fs::{File, Stdin, Stdout};
use crate::mm::{translated_refmut, MemorySet, KERNEL_SPACE};
use crate::sync::{Condvar, Mutex, Semaphore, UPSafeCell};
use crate::task::current_task;
use crate::trap::{trap_handler, TrapContext};
use alloc::string::String;
use alloc::sync::{Arc, Weak};
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefMut;
use core::fmt::{Debug, Formatter};

/// Process Control Block
pub struct ProcessControlBlock {
    /// immutable
    pub pid: PidHandle,
    /// mutable
    inner: UPSafeCell<ProcessControlBlockInner>,
}

/// Inner of Process Control Block
pub struct ProcessControlBlockInner {
    /// is zombie?
    pub is_zombie: bool,
    /// memory set(address space)
    pub memory_set: MemorySet,
    /// parent process
    pub parent: Option<Weak<ProcessControlBlock>>,
    /// children process
    pub children: Vec<Arc<ProcessControlBlock>>,
    /// exit code
    pub exit_code: i32,
    /// file descriptor table
    pub fd_table: Vec<Option<Arc<dyn File + Send + Sync>>>,
    /// signal flags
    pub signals: SignalFlags,
    /// tasks(also known as threads)
    pub tasks: Vec<Option<Arc<TaskControlBlock>>>,
    /// task resource allocator
    pub task_res_allocator: RecycleAllocator,
    /// mutex list
    pub mutex_list: Vec<Option<Arc<dyn Mutex>>>,
    /// mutex dead lock detect
    pub mutex_deadlock_detect: MutexDeadlockDetect,
    /// semaphore list
    pub semaphore_list: Vec<Option<Arc<Semaphore>>>,
    /// semaphore dead lock detect
    pub semaphore_deadlock_detect: SemaphoreDeadlockDetect,
    /// condvar list
    pub condvar_list: Vec<Option<Arc<Condvar>>>,
}

impl ProcessControlBlockInner {
    #[allow(unused)]
    /// get the address of app's page table
    pub fn get_user_token(&self) -> usize {
        self.memory_set.token()
    }
    /// allocate a new file descriptor
    pub fn alloc_fd(&mut self) -> usize {
        if let Some(fd) = (0..self.fd_table.len()).find(|fd| self.fd_table[*fd].is_none()) {
            fd
        } else {
            self.fd_table.push(None);
            self.fd_table.len() - 1
        }
    }
    /// allocate a new task id
    pub fn alloc_tid(&mut self) -> usize {
        self.task_res_allocator.alloc()
    }
    /// deallocate a task id
    pub fn dealloc_tid(&mut self, tid: usize) {
        self.task_res_allocator.dealloc(tid)
    }
    /// the count of tasks(threads) in this process
    pub fn thread_count(&self) -> usize {
        self.tasks.len()
    }
    /// get a task with tid in this process
    pub fn get_task(&self, tid: usize) -> Arc<TaskControlBlock> {
        self.tasks[tid].as_ref().unwrap().clone()
    }
}

impl ProcessControlBlock {
    /// inner_exclusive_access
    pub fn inner_exclusive_access(&self) -> RefMut<'_, ProcessControlBlockInner> {
        self.inner.exclusive_access()
    }
    /// new process from elf file
    pub fn new(elf_data: &[u8]) -> Arc<Self> {
        trace!("kernel: ProcessControlBlock::new");
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, ustack_base, entry_point) = MemorySet::from_elf(elf_data);
        // allocate a pid
        let pid_handle = pid_alloc();
        let process = Arc::new(Self {
            pid: pid_handle,
            inner: unsafe {
                UPSafeCell::new(ProcessControlBlockInner {
                    is_zombie: false,
                    memory_set,
                    parent: None,
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: vec![
                        // 0 -> stdin
                        Some(Arc::new(Stdin)),
                        // 1 -> stdout
                        Some(Arc::new(Stdout)),
                        // 2 -> stderr
                        Some(Arc::new(Stdout)),
                    ],
                    signals: SignalFlags::empty(),
                    tasks: Vec::new(),
                    task_res_allocator: RecycleAllocator::new(),
                    mutex_list: Vec::new(),
                    mutex_deadlock_detect: MutexDeadlockDetect::new(0),
                    semaphore_list: Vec::new(),
                    semaphore_deadlock_detect: SemaphoreDeadlockDetect::new(),
                    condvar_list: Vec::new(),
                })
            },
        });
        // create a main thread, we should allocate ustack and trap_cx here
        let task = Arc::new(TaskControlBlock::new(
            Arc::clone(&process),
            ustack_base,
            true,
        ));
        // prepare trap_cx of main thread
        let task_inner = task.inner_exclusive_access();
        let trap_cx = task_inner.get_trap_cx();
        let ustack_top = task_inner.res.as_ref().unwrap().ustack_top();
        let kstack_top = task.kstack.get_top();
        drop(task_inner);
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            ustack_top,
            KERNEL_SPACE.exclusive_access().token(),
            kstack_top,
            trap_handler as usize,
        );
        // add main thread to the process
        let mut process_inner = process.inner_exclusive_access();
        process_inner.tasks.push(Some(Arc::clone(&task)));
        drop(process_inner);
        insert_into_pid2process(process.getpid(), Arc::clone(&process));
        // add main thread to scheduler
        add_task(task);
        process
    }

    /// Only support processes with a single thread.
    pub fn exec(self: &Arc<Self>, elf_data: &[u8], args: Vec<String>) {
        trace!("kernel: exec");
        assert_eq!(self.inner_exclusive_access().thread_count(), 1);
        // memory_set with elf program headers/trampoline/trap context/user stack
        trace!("kernel: exec .. MemorySet::from_elf");
        let (memory_set, ustack_base, entry_point) = MemorySet::from_elf(elf_data);
        let new_token = memory_set.token();
        // substitute memory_set
        trace!("kernel: exec .. substitute memory_set");
        self.inner_exclusive_access().memory_set = memory_set;
        // then we alloc user resource for main thread again
        // since memory_set has been changed
        trace!("kernel: exec .. alloc user resource for main thread again");
        let task = self.inner_exclusive_access().get_task(0);
        let mut task_inner = task.inner_exclusive_access();
        task_inner.res.as_mut().unwrap().ustack_base = ustack_base;
        task_inner.res.as_mut().unwrap().alloc_user_res();
        task_inner.trap_cx_ppn = task_inner.res.as_mut().unwrap().trap_cx_ppn();
        // push arguments on user stack
        trace!("kernel: exec .. push arguments on user stack");
        let mut user_sp = task_inner.res.as_mut().unwrap().ustack_top();
        user_sp -= (args.len() + 1) * core::mem::size_of::<usize>();
        let argv_base = user_sp;
        let mut argv: Vec<_> = (0..=args.len())
            .map(|arg| {
                translated_refmut(
                    new_token,
                    (argv_base + arg * core::mem::size_of::<usize>()) as *mut usize,
                )
            })
            .collect();
        *argv[args.len()] = 0;
        for i in 0..args.len() {
            user_sp -= args[i].len() + 1;
            *argv[i] = user_sp;
            let mut p = user_sp;
            for c in args[i].as_bytes() {
                *translated_refmut(new_token, p as *mut u8) = *c;
                p += 1;
            }
            *translated_refmut(new_token, p as *mut u8) = 0;
        }
        // make the user_sp aligned to 8B for k210 platform
        user_sp -= user_sp % core::mem::size_of::<usize>();
        // initialize trap_cx
        trace!("kernel: exec .. initialize trap_cx");
        let mut trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            task.kstack.get_top(),
            trap_handler as usize,
        );
        trap_cx.x[10] = args.len();
        trap_cx.x[11] = argv_base;
        *task_inner.get_trap_cx() = trap_cx;
    }

    /// Only support processes with a single thread.
    pub fn fork(self: &Arc<Self>) -> Arc<Self> {
        trace!("kernel: fork");
        let mut parent = self.inner_exclusive_access();
        assert_eq!(parent.thread_count(), 1);
        // clone parent's memory_set completely including trampoline/ustacks/trap_cxs
        let memory_set = MemorySet::from_existed_user(&parent.memory_set);
        // alloc a pid
        let pid = pid_alloc();
        // copy fd table
        let mut new_fd_table: Vec<Option<Arc<dyn File + Send + Sync>>> = Vec::new();
        for fd in parent.fd_table.iter() {
            if let Some(file) = fd {
                new_fd_table.push(Some(file.clone()));
            } else {
                new_fd_table.push(None);
            }
        }
        // create child process pcb
        let child = Arc::new(Self {
            pid,
            inner: unsafe {
                UPSafeCell::new(ProcessControlBlockInner {
                    is_zombie: false,
                    memory_set,
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: new_fd_table,
                    signals: SignalFlags::empty(),
                    tasks: Vec::new(),
                    task_res_allocator: RecycleAllocator::new(),
                    mutex_list: Vec::new(),
                    mutex_deadlock_detect: MutexDeadlockDetect::new(0),
                    semaphore_list: Vec::new(),
                    semaphore_deadlock_detect: SemaphoreDeadlockDetect::new(),
                    condvar_list: Vec::new(),
                })
            },
        });
        // add child
        parent.children.push(Arc::clone(&child));
        // create main thread of child process
        let task = Arc::new(TaskControlBlock::new(
            Arc::clone(&child),
            parent
                .get_task(0)
                .inner_exclusive_access()
                .res
                .as_ref()
                .unwrap()
                .ustack_base(),
            // here we do not allocate trap_cx or ustack again
            // but mention that we allocate a new kstack here
            false,
        ));
        // attach task to child process
        let mut child_inner = child.inner_exclusive_access();
        child_inner.tasks.push(Some(Arc::clone(&task)));
        drop(child_inner);
        // modify kstack_top in trap_cx of this thread
        let task_inner = task.inner_exclusive_access();
        let trap_cx = task_inner.get_trap_cx();
        trap_cx.kernel_sp = task.kstack.get_top();
        drop(task_inner);
        insert_into_pid2process(child.getpid(), Arc::clone(&child));
        // add this thread to scheduler
        add_task(task);
        child
    }
    /// get pid
    pub fn getpid(&self) -> usize {
        self.pid.0
    }
}

pub struct MutexDeadlockDetect {
    /// dead lock detect enable?
    deadlock_detect: bool,
    available: Vec<i32>,
    allocation: Vec<Vec<i32>>,
    need: Vec<Vec<i32>>,
}

impl MutexDeadlockDetect {
    pub fn new(n: usize) -> Self {
        // 初始放一个资源
        let available = vec![1; n + 1];
        let allocation = vec![vec![0; n]; n];
        let need = vec![vec![0; n]; n];
        MutexDeadlockDetect {
            deadlock_detect: false,
            available,
            allocation,
            need,
        }
    }
    pub fn open(&mut self) {
        debug!("enable deadlock detect");
        self.deadlock_detect = true;
    }
    pub fn close(&mut self) {
        debug!("disable deadlock detect");
        self.deadlock_detect = false;
    }

    pub fn consume_resource(&mut self, resource: usize, num: usize) {
        if !self.deadlock_detect {
            return;
        }
        self.available[resource] -= num as i32;
    }

    pub fn allocate_resource(&mut self, task: usize, resource: Vec<i32>) {
        if !self.deadlock_detect {
            return;
        }
        if task >= self.allocation.len() {
            self.allocation.push(resource);
            return;
        }
        assert!(task == self.allocation.len() - 1);
        self.allocation[task] = resource;
    }

    pub fn need_resource(&mut self, task: usize, resource: Vec<i32>) {
        if !self.deadlock_detect {
            return;
        }
        if task >= self.need.len() {
            self.need.push(resource);
            return;
        }
        self.need[task] = resource;
    }
    pub fn detect(&self) -> bool {
        debug!("{:?}", self);
        if !self.deadlock_detect {
            info!("detect deadlock detect is not enabled");
            true;
        }
        info!("deadlock detect");
        debug!("now processor have {} source", self.available.len());
        debug!("now processor have {} task", self.allocation.len());
        // work = available
        let mut work = self.available.clone();
        let mut finish = vec![false; self.allocation.len()];

        let mut flag = true;
        while flag {
            flag = false;
            for i in 0..self.allocation.len() {
                if !finish[i] && self.need[i].iter().zip(work.iter()).all(|(x, y)| x <= y) {
                    finish[i] = true;
                    flag = true;
                    for j in 0..self.available.len() {
                        work[j] += self.allocation[i][j];
                    }
                }
            }
        }
        debug!("finish {:?}", finish);
        finish.iter().all(|x| *x)
    }
}

impl Debug for MutexDeadlockDetect {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MutexDeadlockDetect")
            .field("deadlock_detect", &self.deadlock_detect)
            .field("available", &self.available)
            .field("allocation", &self.allocation)
            .field("need", &self.need)
            .finish()
    }
}

pub struct SemaphoreDeadlockDetect {
    /// dead lock detect enable?
    deadlock_detect: bool,
    available: Vec<i32>,
    allocation: Vec<Vec<i32>>,
    need: Vec<Vec<i32>>,
}

impl Debug for SemaphoreDeadlockDetect {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        // 手动输出 available 数组，逐行输出
        writeln!(f, "available:")?;
        writeln!(f, "           {:?}", self.available)?;
        // 手动输出 allocation 数组，逐行输出
        writeln!(f, "allocation:")?;
        for (i, row) in self.allocation.iter().enumerate() {
            writeln!(f, "thread[{}]: {:?}", i, row)?;
        }
        // 手动输出 need 数组，逐行输出
        writeln!(f, "need:")?;
        for (i, row) in self.need.iter().enumerate() {
            writeln!(f, "thread[{}]: {:?}", i, row)?;
        }
        Ok(())
    }
}

impl SemaphoreDeadlockDetect {
    pub fn new() -> Self {
        // 假设最多只有4个互斥锁资源
        let available = vec![0; 5];
        let allocation = vec![vec![0; 5]; 5];
        let need = vec![vec![0; 5]; 5];
        SemaphoreDeadlockDetect {
            deadlock_detect: false,
            available,
            allocation,
            need,
        }
    }

    pub fn open(&mut self) {
        debug!("enable deadlock detect");
        self.deadlock_detect = true;
    }
    pub fn close(&mut self) {
        debug!("disable deadlock detect");
        self.deadlock_detect = false;
    }

    /// 消费掉总资源
    pub fn consume_resource(&mut self, resource: usize, num: usize) {
        if !self.deadlock_detect {
            return;
        }
        self.available[resource] -= num as i32;
    }

    /// 总资源增加
    pub fn put_resource(&mut self, resource: usize, num: usize) {
        if !self.deadlock_detect {
            return;
        }
        self.available[resource] += num as i32;
    }

    /// 分配资源
    pub fn allocate_resource(&mut self, task: usize, resource_id: usize, num: usize) {
        if !self.deadlock_detect {
            return;
        }
        assert!(resource_id < 5);
        assert!(task < 5);
        self.allocation[task][resource_id] += num as i32;
    }

    /// 回收已经分配的资源
    pub fn deallocate_resource(&mut self, task: usize, resource_id: usize, num: usize) {
        if !self.deadlock_detect {
            return;
        }
        assert!(task < self.allocation.len());
        assert!(resource_id < 5);
        self.allocation[task][resource_id] -= num as i32;
    }
    /// 需要资源
    pub fn need_num_resource(&mut self, task: usize, resource_id: usize, num: usize) {
        if !self.deadlock_detect {
            return;
        }
        assert!(task < 5);
        assert!(resource_id < 5);
        self.need[task][resource_id] += num as i32;
    }

    /// 释放需要的资源
    pub fn release_need_num_resource(&mut self, task: usize, resource_id: usize, num: usize) {
        if !self.deadlock_detect {
            return;
        }
        assert!(task < 5);
        assert!(resource_id < 5);
        self.need[task][resource_id] -= num as i32;
    }

    /// 检测死锁
    pub fn detect(&self) -> bool {
        if !self.deadlock_detect {
            info!("detect deadlock detect is not enabled");
            return true;
        }
        let tid = current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid;
        info!("deadlock detect");
        debug!("********{}**********", tid);
        debug!("{:?}", self);
        debug!("**********************");
        // work = available
        let mut work = self.available.clone();
        let mut finish = vec![false; self.allocation.len()];
        finish[0] = true;

        let mut flag = true;
        while flag {
            flag = false;
            for i in 0..self.allocation.len() {
                if !finish[i] && self.need[i].iter().zip(work.iter()).all(|(x, y)| x <= y) {
                    finish[i] = true;
                    flag = true;
                    for j in 0..self.available.len() {
                        work[j] += self.allocation[i][j];
                    }
                }
            }
        }
        debug!("finish {:?}", finish);
        finish.iter().all(|x| *x)
    }
}
