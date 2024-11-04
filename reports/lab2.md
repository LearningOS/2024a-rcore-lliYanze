# 实验2报告

## 荣誉准则

在完成本次实验的过程（含此前学习的过程）中，我曾分别与 以下各位 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

无

此外，我也参考了 以下资料 ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

<https://learningos.cn/rCore-Camp-Guide-2024A/chapter4/3sv39-implementation-1.html>

微信群关于user mode, 以及page fault的讨论

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。

## 编程题

首先 sys_taskinfo 和 sys_time原理一样：
都是传入的是虚拟地址，但是需要得到其物理地址，给其物理地址赋上所需要的值才能正确的返回所需要的值

mmap:
给pcb的内存管理加入一个专门存放这种匿名页的maparea，同时在mmap的时候需要先进行一些判断合法性，然后再调用maparea的`map`来给start到end进行map到物理地址(都是Frameallocter管理),需要注意的就是map的权限的设置，以及需要设置User模式,还有一个点**检查vpn的有效性的时候要通过对应pte的valid来判断,主要是unmap并不是将整个maparea删除，而是将其中一部分unmap，并且unmap并不在意maparea是否会有重叠，所以只能通过最后映射的页的valid来判断**

unmap:
mmap实现了后，unmap就非常简单了，就是将vpn的pte判断是valid后，通过pagetable的unmap进行释放(就是将pte无效)

## 简答题

1.请列举 SV39 页表页表项的组成，描述其中的标志位有何作用？

组成:

```
[64:54] [53:10] [9:8] [7:0]
 |保留|   |ppn|  |rsw| |标志位|
```

标志位的作用可以通过下面的代码段看出

```rust
/// page table entry flags
pub struct PTEFlags: u8 {
 /// Valid
 const V = 1 << 0;
 /// Readable
 const R = 1 << 1;
 /// Writable
 const W = 1 << 2;
 /// Executable
 const X = 1 << 3;
 /// User mode accessible
 const U = 1 << 4;
 /// Global
 const G = 1 << 5;
 /// Accessed
 const A = 1 << 6;
 /// Dirty
 const D = 1 << 7;
}
```

2. 缺页

2.1 请问哪些异常可能是缺页导致的？

- not valid
- 模式不匹配 U时访问S
- 读写不匹配

2.2 发生缺页时，描述相关重要寄存器的值，上次实验描述过的可以简略。

- csr寄存器的状态
- mcause设置成缺页异常
- mepc设置成返回地址
- satp 控制模式
- pc

2.3 这样做有哪些好处？

>可以在text等这些段在使用到的时候才会去分配内存进行填充，这样可以节省不必要分配的空间，并且可以缩短程序加载的时间

2.4 处理 10G 连续的内存页面，对应的 SV39 页表大致占用多少内存 (估算数量级即可)？

>10g需要用 $10*2^{19}$ 个页,每个页都有一个页表项，一个页表项8字节，所以需要$10*2^{19+3-10-10}mb$ 大概40mb

2.5 请简单思考如何才能实现 Lazy 策略，缺页时又如何处理？描述合理即可，不需要考虑实现。
>将在缺页异常中进行处理，如果在预留分配的空间内，但是没有map，就将该虚拟页进行映射再给程序使用

2.6 此时页面失效如何表现在页表项(PTE)上？
>v此时是无效，可以指示该页可能是因为swap引起的无效，可以增加标志位，或者增加全局的swap管理来进一步表现

3.双页表与单页表

3.1 在单页表情况下，如何更换页表？
>只要将此时的状态换掉，并不需要更换root

 3.2单页表情况下，如何控制用户态无法访问内核页面？（tips:看看上一题最后一问）

     通过pte状态

3.3 单页表有何优势？（回答合理即可）
>切换更快，内陷不需要将tlb清空

3.4双页表实现下，何时需要更换页表？假设你写一个单页表操作系统，你会选择何时更换页表（回答合理即可）？
>双：内陷 进程切换
>
>单：进程切换