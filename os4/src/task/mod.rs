//! Task management implementation
//!
//! Everything about task management, like starting and switching tasks is
//! implemented here.
//!
//! A single global instance of [`TaskManager`] called `TASK_MANAGER` controls
//! all the tasks in the operating system.
//!
//! Be careful when you see [`__switch`]. Control flow around this function
//! might not be what you expect.

mod context;
mod switch;
#[allow(clippy::module_inception)]
mod task;
use crate::config::{MAX_SYSCALL_NUM, PAGE_SIZE};
use crate::loader::{get_app_data, get_num_app};
use crate::sync::UPSafeCell;
use crate::trap::TrapContext;
use crate::mm::{VirtAddr, VirtPageNum, MapPermission, MemorySet, VPNRange};
use alloc::vec;
use alloc::vec::Vec;
use lazy_static::*;
pub use switch::__switch;
pub use task::{TaskControlBlock, TaskStatus, TaskSysCallTimes};
use crate::timer::get_time_us;
pub use context::TaskContext;

/// The task manager, where all the tasks are managed.
///
/// Functions implemented on `TaskManager` deals with all task state transitions
/// and task context switching. For convenience, you can find wrappers around it
/// in the module level.
///
/// Most of `TaskManager` are hidden behind the field `inner`, to defer
/// borrowing checks to runtime. You can see examples on how to use `inner` in
/// existing functions on `TaskManager`.
pub struct TaskManager {
    /// total number of tasks
    num_app: usize,
    /// use inner value to get mutable access
    inner: UPSafeCell<TaskManagerInner>,
}

/// The task manager inner in 'UPSafeCell'
struct TaskManagerInner {
    /// task list
    tasks: Vec<TaskControlBlock>,
    syscall_times_record: Vec<TaskSysCallTimes>,
    /// id of current `Running` task
    current_task: usize,
}

lazy_static! {
    /// a `TaskManager` instance through lazy_static!
    pub static ref TASK_MANAGER: TaskManager = {
        info!("init TASK_MANAGER");
        let num_app = get_num_app();
        info!("num_app = {}", num_app);
        let mut tasks: Vec<TaskControlBlock> = Vec::new();
        for i in 0..num_app {
            tasks.push(TaskControlBlock::new(get_app_data(i), i));
        }
        let mut tasks_syscall_times = vec![TaskSysCallTimes{
            syscall_times: vec![0; MAX_SYSCALL_NUM],
        }; num_app];
        TaskManager {
            num_app,
            inner: unsafe {
                UPSafeCell::new(TaskManagerInner {
                    tasks,
                    syscall_times_record: tasks_syscall_times,
                    current_task: 0,
                })
            },
        }
    };
}

impl TaskManager {
    /// Run the first task in task list.
    ///
    /// Generally, the first task in task list is an idle task (we call it zero process later).
    /// But in ch4, we load apps statically, so the first task is a real app.
    fn run_first_task(&self) -> ! {
        let mut inner = self.inner.exclusive_access();
        let next_task = &mut inner.tasks[0];
        next_task.task_status = TaskStatus::Running;
        let next_task_cx_ptr = &next_task.task_cx as *const TaskContext;
        if next_task.first_running_time == 0 {
            next_task.first_running_time = get_time_us() / 1000;
        }
        drop(inner);
        let mut _unused = TaskContext::zero_init();
        // before this, we should drop local variables that must be dropped manually
        unsafe {
            __switch(&mut _unused as *mut _, next_task_cx_ptr);
        }
        panic!("unreachable in run_first_task!");
    }

    /// Change the status of current `Running` task into `Ready`.
    fn mark_current_suspended(&self) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].task_status = TaskStatus::Ready;
    }

    /// Change the status of current `Running` task into `Exited`.
    fn mark_current_exited(&self) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].task_status = TaskStatus::Exited;
    }

    /// Find next task to run and return task id.
    ///
    /// In this case, we only return the first `Ready` task in task list.
    fn find_next_task(&self) -> Option<usize> {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        (current + 1..current + self.num_app + 1)
            .map(|id| id % self.num_app)
            .find(|id| inner.tasks[*id].task_status == TaskStatus::Ready)
    }

    /// Get the current 'Running' task's token.
    fn get_current_token(&self) -> usize {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.current_task].get_user_token()
    }

    #[allow(clippy::mut_from_ref)]
    /// Get the current 'Running' task's trap contexts.
    fn get_current_trap_cx(&self) -> &mut TrapContext {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.current_task].get_trap_cx()
    }

    /// Switch current `Running` task to the task we have found,
    /// or there is no `Ready` task and we can exit with all applications completed
    fn run_next_task(&self) {
        if let Some(next) = self.find_next_task() {
            let mut inner = self.inner.exclusive_access();
            let current = inner.current_task;
            inner.tasks[next].task_status = TaskStatus::Running;
            inner.current_task = next;
            if inner.tasks[next].first_running_time == 0 {
                inner.tasks[next].first_running_time = get_time_us() / 1000;
            }
            let current_task_cx_ptr = &mut inner.tasks[current].task_cx as *mut TaskContext;
            let next_task_cx_ptr = &inner.tasks[next].task_cx as *const TaskContext;
            drop(inner);
            // before this, we should drop local variables that must be dropped manually
            unsafe {
                __switch(current_task_cx_ptr, next_task_cx_ptr);
            }
            // go back to user mode
        } else {
            panic!("All applications completed!");
        }
    }
    fn get_current_task_status(&self) -> TaskStatus {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].task_status
    }
    fn get_current_task_first_time(&self) -> usize {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].first_running_time
    }
    fn update_current_task_syscall_times(&self, syscall_id: usize) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.syscall_times_record[current].syscall_times[syscall_id] += 1;

    }
    fn get_current_task_times(&self) -> Vec<u32> {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.syscall_times_record[current].syscall_times.clone()
    }
    fn mmap(&self, start: usize, len: usize, port: usize) -> isize {
        if start & (PAGE_SIZE - 1) != 0 {
            return -1;
        }
        // port最低三位[x w r]，其他位必须为0
        if port > 7usize || port == 0 {
            return -1;
        }

        let mut inner = self.inner.exclusive_access();
        let task_id = inner.current_task;
        let current_task = &mut inner.tasks[task_id];
        let memory_set = &mut current_task.memory_set;
  
        // check valid
        let start_vpn = VirtPageNum::from(VirtAddr(start));
        let end_vpn = VirtPageNum::from(VirtAddr(start + len).ceil());
        for vpn in start_vpn.0 .. end_vpn.0 {
            if let Some(pte) = memory_set.translate(VirtPageNum(vpn)) {
                if pte.is_valid() {
                    return -1;
                }
            }
        }
        let permission = MapPermission::from_bits((port as u8) << 1).unwrap() | MapPermission::U;
        memory_set.insert_framed_area(VirtAddr(start), VirtAddr(start+len), permission);
        0
    }
    fn munmap(&self, start: usize, len: usize) -> isize {
        if start & (PAGE_SIZE - 1) != 0 {
            return -1;
        }
      
        let mut inner = self.inner.exclusive_access();
        let task_id = inner.current_task;
        let current_task = &mut inner.tasks[task_id];
        let memory_set = &mut current_task.memory_set;

        // check valid
        let start_vpn = VirtPageNum::from(VirtAddr(start));
        let end_vpn = VirtPageNum::from(VirtAddr(start + len).ceil());
        for vpn in start_vpn.0 .. end_vpn.0 {
            if let Some(pte) = memory_set.translate(VirtPageNum(vpn)) {
                if !pte.is_valid() {
                    return -1;
                }
            }
        }
        
        let vpn_range = VPNRange::new(start_vpn, end_vpn);
        memory_set.munmap(vpn_range);
        0
    }
}

pub fn get_current_task_status() -> TaskStatus {
    TASK_MANAGER.get_current_task_status()
}
pub fn get_current_task_first_time() -> usize {
    TASK_MANAGER.get_current_task_first_time()
}
pub fn update_current_task_syscall_times(syscall_id: usize) {
    TASK_MANAGER.update_current_task_syscall_times(syscall_id);
}
pub fn get_syscall_times() -> [u32; MAX_SYSCALL_NUM] {
    let syscall_times = TASK_MANAGER.get_current_task_times();
    let mut res: [u32; MAX_SYSCALL_NUM] = [0; MAX_SYSCALL_NUM];
    let mut index: usize = 0;
    while index < MAX_SYSCALL_NUM {
        res[index] = syscall_times[index];
        index += 1;
    }
    res
}
pub fn current_task_mmap(start: usize, len: usize, port: usize) -> isize {
    TASK_MANAGER.mmap(start, len, port)
}
pub fn current_task_munmap(start: usize, len: usize) -> isize {
    TASK_MANAGER.munmap(start, len)
}
/// Run the first task in task list.
pub fn run_first_task() {
    TASK_MANAGER.run_first_task();
}

/// Switch current `Running` task to the task we have found,
/// or there is no `Ready` task and we can exit with all applications completed
fn run_next_task() {
    TASK_MANAGER.run_next_task();
}

/// Change the status of current `Running` task into `Ready`.
fn mark_current_suspended() {
    TASK_MANAGER.mark_current_suspended();
}

/// Change the status of current `Running` task into `Exited`.
fn mark_current_exited() {
    TASK_MANAGER.mark_current_exited();
}

/// Suspend the current 'Running' task and run the next task in task list.
pub fn suspend_current_and_run_next() {
    mark_current_suspended();
    run_next_task();
}

/// Exit the current 'Running' task and run the next task in task list.
pub fn exit_current_and_run_next() {
    mark_current_exited();
    run_next_task();
}

/// Get the current 'Running' task's token.
pub fn current_user_token() -> usize {
    TASK_MANAGER.get_current_token()
}

/// Get the current 'Running' task's trap contexts.
pub fn current_trap_cx() -> &'static mut TrapContext {
    TASK_MANAGER.get_current_trap_cx()
}
