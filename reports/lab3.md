# lab3实验报告

## 荣誉准则

1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与 mochi dunamic_pigeon就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

    stide算法部分

2. 此外，我也参考了 以下资料 ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

    <https://learningos.cn/rCore-Camp-Guide-2024A/chapter4/3sv39-implementation-1.html>
    <https://www.inlighting.org/archives/cpu-scheduling-policies>

    微信群关于stride的讨论

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。

## 编程作业

### spawn

这个部分其实比较简单，spawn的目的是不借助`fork`制作一个进程，所以可以使用TaskControl的`new` 方法直接从path中将elf文件内容读取出来创建一个进程空间就好，最后处理一下父子进程关系，将创建好的进程`add_task`就可了

### stride算法

这个部分其实更简单了，题目描述的十分清晰，将pcb增加stride字段，并且在fetch中将`ready`的任务中，stride中最小的拿出来就好了

## 问答作业

1. 实际情况是轮到 p1 执行吗？为什么？

    >不是,由于是8bit，p2执行后发生了溢出，stride会变成5导致依旧会调用p2，并且再次运行

2. 我们之前要求进程优先级 >= 2 其实就是为了解决这个问题。可以证明， 在不考虑溢出的情况下 , 在进程优先级全部 >= 2 的情况下，如果严格按照算法执行，那么 STRIDE_MAX – STRIDE_MIN <= BigStride / 2。 为什么？尝试简单说明（不要求严格证明）。
    > 讨论后结果：由于pass最大就是BigStride/2，所以不可能会出现STRIDE_MAX – STRIDE_MIN > BigStride / 2的情况

3. 重新设计Stride
其实就是用最高位来代表是否发生溢出，可以将其变成isize来作比较

    ```rust
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.0 as i64).cmp(other.0 as i64)
    }

    ```
