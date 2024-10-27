# lab1实验报告

## 编程题

中途也有很多的波折,踩到的值得记录的坑

- 注意不要更改测试文件中的结构
- `TaskInfo`不需要放到`TaskManager`中改变，可以在`TaskControlBlock`,中加入下面几个成员记录，最后在syscall中将内容拼接成`TaskInfo`就可以
- `TaskManager`的trait不可以用 `&mut self` 只能用 `&self`,但是可以将其中的`inner`变成mut的引用来改变其值。
- 更新run time的时间节点
  - 第一次调度时设置好 `start_time`
  - 需要读取`TaskInfo`时,更新一次`time`

最后几个重点改动

```rust
// pcb
pub struct TaskControlBlock {
    ...
    /// The number of syscalls called by the task
    pub syscall_times: [u32; MAX_SYSCALL_NUM],
    /// The total running time of the task
    pub time: usize,
    /// the start time of the task
    pub start_time: usize,
}

// 更新sys call
pub fn syscall(syscall_id: usize, args: [usize; 3]) -> isize {
    TASK_MANAGER.update_current_syscall_times(syscall_id);
    match syscall_id {
        ...
    }
    ...
}


pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    debug!("kernel: sys_task_info");
    // 跟新run time
    TASK_MANAGER.update_run_time();
    unsafe {
        *_ti = TaskInfo {
            status: TASK_MANAGER.get_current_task_status(),
            syscall_times: TASK_MANAGER.get_current_task_syscall_times(),
            time: TASK_MANAGER.get_current_task_time(),
        };
    };
    0
}
```

给`TaskManager`增加几个trait

```rust
    /// update the syscall times of current task
    pub fn update_current_syscall_times(&self, syscall_id: usize) {
        if syscall_id >= MAX_SYSCALL_NUM {
            warn!("Unsupported syscall_id: {}", syscall_id);
            return;
        }
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].syscall_times[syscall_id] += 1;
    }

    /// update run time
    pub fn update_run_time(&self) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].update_run_time();
    }

    /// get the status of current task
    pub fn get_current_task_status(&self) -> TaskStatus {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].task_status
    }

    /// get the syscall times of current task
    pub fn get_current_task_syscall_times(&self) -> [u32; MAX_SYSCALL_NUM] {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].syscall_times
    }

    /// get the total running time of current task
    pub fn get_current_task_time(&self) -> usize {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].time
    }
```

## 简答题

>正确进入 U 态后，程序的特征还应有：使用 S 态特权指令，访问 S 态寄存器后会报错。 请同学们可以自行测试这些内容（运行 三个 bad 测例 (ch2b\_bad\_\*.rs) ）， 描述程序出错行为，同时注意注明你使用的 sbi 及其版本

第一个：直接往一个地址写入，由于当前的地址还没有被分配，所以直接写入会出现pagefault。

第二个：直接执行mret，当前在U状态，没有权限，造成错误

第三个：基本的原理和第二个差不多，都是没有权限，直接去操作了csr寄存器，导致错误

>
>深入理解 [trap.S](https://github.com/LearningOS/rCore-Tutorial-Code-2024S/blob/ch3/os/src/trap/trap.S) 中两个函数 `__alltraps` 和 `__restore` 的作用，并回答如下问题:
>
>1. L40：刚进入 `__restore` 时，`a0` 代表了什么值。请指出 `__restore` 的两种使用情景。
>
>2. L43-L48：这几行汇编代码特殊处理了哪些寄存器？这些寄存器的的值对于进入用户态有何意义？请分别解释。
>
>```asm
>    ld t0, 32*8(sp)
>    ld t1, 33*8(sp)
>    ld t2, 2*8(sp)
>    csrw sstatus, t0
>    csrw sepc, t1
>    csrw sscratch, t2
>```
>
>3.L50-L56：为何跳过了 `x2` 和 `x4`？
>
> ```asm
>     ld x1, 1*8(sp)
>     ld x3, 3*8(sp)
>     .set n, 5
>     .rept 27
>        LOAD_GP %n
>        .set n, n+1
>     .endr
> ```
>
>4.L60：该指令之后，`sp` 和 `sscratch` 中的值分别有什么意义？
>
> ```asm
> csrrw sp, sscratch, sp
> ```
>
>5.`__restore`：中发生状态切换在哪一条指令？为何该指令执行之后会进入用户态？
>
>6.L13：该指令之后，`sp` 和 `sscratch` 中的值分别有什么意义？
>
> ```asm
> csrrw sp, sscratch, sp
> ```
>
>7.从 U 态进入 S 态是哪一条指令发生的？

1.由于`__restore`没有参数传递，所以a0中还是 `goto_restore` 中的参数 `kstack_ptr`
使用场景：

- 中断返回出栈上的上下文
- 从异常返回

2.专门处理了几个csr寄存器，通过这个几个寄存器可以知道返回值、触发切换的原因

- t0 被加载为 sstatus，用于恢复处理器的状态寄存器。
- t1 被加载为 sepc，这是程序计数器，用于恢复代码执行的地址。
- t2 被加载为 sscratch，通常保存用户栈指针。

3.x2是sp，不需要特殊保存处理，后续返回的时候就会到正确的位置
x4是线程指针，不在这里处理

4.sp和sscatch本来是内核栈指针和内核指针，这里将两者交换。将sp换成用户态的栈指针，为后面退出到用户态做准备

5.`mret` 返回的时候会发生状态改变

6.从用户态进来的时候sp是用户态的栈指针，这里是交换变成内核态的指针

7.`csrrw sp, sscratch, sp`
