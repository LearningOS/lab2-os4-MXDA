//! Process management syscalls

use crate::config::MAX_SYSCALL_NUM;
use crate::task::{exit_current_and_run_next, suspend_current_and_run_next, TaskStatus, get_current_task_first_time,
    get_current_task_status, get_syscall_times, current_user_token, current_task_mmap, current_task_munmap};
use crate::timer::get_time_us;

use crate::mm::{VirtAddr2PhysAddr, VirtAddr};
#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

#[derive(Clone, Copy)]
pub struct TaskInfo {
    pub status: TaskStatus,
    pub syscall_times: [u32; MAX_SYSCALL_NUM],
    pub time: usize,
}

pub fn sys_exit(exit_code: i32) -> ! {
    info!("[kernel] Application exited with code {}", exit_code);
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    suspend_current_and_run_next();
    0
}
//fn VirtAddr2PhysAddr(va: VirtAddr) -> Option<PhysAddr> {
//    
//}

// YOUR JOB: 引入虚地址后重写 sys_get_time
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    /*let _us = get_time_us();
    let current_token = current_user_token();
    let ts = VirtAddr2PhysAddr(current_token, _ts as *const u8) as *mut TimeVal;
    unsafe {
        *ts = TimeVal {
            sec: _us / 1_000_000,
            usec: _us % 1_000_000, 
        };
    }
    0
    */
    let va = VirtAddr(ts as usize);
    if let Some(pa) = VirtAddr2PhysAddr(va) {
        let us = get_time_us();
        let physAddr_ts = pa.0 as *mut TimeVal;
        unsafe {
            *physAddr_ts = TimeVal {
                sec: us / 1_000_000,
                usec: us % 1_000_000,
            };
        }
        0
    } else {
        -1
    }
}

// CLUE: 从 ch4 开始不再对调度算法进行测试~
pub fn sys_set_priority(_prio: isize) -> isize {
    -1
}

// YOUR JOB: 扩展内核以实现 sys_mmap 和 sys_munmap
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    current_task_mmap(_start, _len, _port)
}

pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    current_task_munmap(_start, _len)
}

// YOUR JOB: 引入虚地址后重写 sys_task_info
pub fn sys_task_info(ti: *mut TaskInfo) -> isize {
    /*let current_token = current_user_token();
    let _ti = VirtAddr2PhysAddr(current_token, ti as *const u8) as *mut TaskInfo;
    unsafe {
        *_ti = TaskInfo {
            status: get_current_task_status(),
            syscall_times: get_syscall_times(),
            time: (get_time_us() / 1000 - get_current_task_first_time()),
        };
    }
    0
    */
    if let Some(pa) = VirtAddr2PhysAddr(VirtAddr(ti as usize)) {
        let pa_ti = pa.0 as *mut TaskInfo;
        unsafe {
        *pa_ti = TaskInfo {
            status: get_current_task_status(),
            syscall_times: get_syscall_times(),
            time: (get_time_us() / 1000 - get_current_task_first_time()),
            };
            0
        }
    } else {
        -1
    }
}
