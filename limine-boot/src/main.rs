//! Limine 引导器入口点
//!
//! 这个 crate 是 Limine 引导器的入口点。
//! 它依赖 kernel-core 和 limine crate。

#![no_std]
#![no_main]

use core::arch::asm;
use core::ptr;
use core::sync::atomic::{AtomicBool, Ordering};

// 引入 Limine crate
use limine::BaseRevision;
use limine::request::{FramebufferRequest, RequestsEndMarker, RequestsStartMarker};

// 引入内核核心库
use kernel_core::boot::{Bootloader, BootInfo};
use kernel_core::boot::info::FramebufferInfo as KernelFramebufferInfo;
use kernel_core::console::{Console, UartConsole};
use kernel_core::Kernel;

/// Limine 基础版本请求
#[used]
#[unsafe(link_section = ".bootloader_requests")]
static BASE_REVISION: BaseRevision = BaseRevision::new();

/// 帧缓冲区请求
#[used]
#[unsafe(link_section = ".bootloader_requests")]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

/// 引导程序请求起始标记
#[used]
#[unsafe(link_section = ".bootloader_requests_start")]
static _START_MARKER: RequestsStartMarker = RequestsStartMarker::new();

/// 引导程序请求结束标记
#[used]
#[unsafe(link_section = ".bootloader_requests_end")]
static _END_MARKER: RequestsEndMarker = RequestsEndMarker::new();

/// 引导信息结构
#[repr(C)]
struct LimineBootInfo {
    framebuffer_addr: *mut u8,
    framebuffer_width: u64,
    framebuffer_height: u64,
    framebuffer_pitch: u64,
    framebuffer_bpp: u16,
    bootloader_name: [u8; 32],
}

/// 全局引导信息
#[used]
#[unsafe(link_section = ".bootloader_info")]
static mut BOOT_INFO: LimineBootInfo = LimineBootInfo {
    framebuffer_addr: ptr::null_mut(),
    framebuffer_width: 0,
    framebuffer_height: 0,
    framebuffer_pitch: 0,
    framebuffer_bpp: 0,
    bootloader_name: [0; 32],
};

/// 初始化标志
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// 初始化引导信息
fn init_boot_info() {
    if INITIALIZED.load(Ordering::SeqCst) {
        return;
    }

    if !BASE_REVISION.is_supported() {
        INITIALIZED.store(true, Ordering::SeqCst);
        return;
    }

    unsafe {
        // 填充引导程序名称
        let name = b"Limine\0";
        let boot_info = &raw mut BOOT_INFO;
        for i in 0..name.len() {
            if i < (*boot_info).bootloader_name.len() {
                (*boot_info).bootloader_name[i] = name[i];
            }
        }

        // 获取帧缓冲区信息
        if let Some(response) = FRAMEBUFFER_REQUEST.get_response() {
            if let Some(framebuffer) = response.framebuffers().next() {
                (*boot_info).framebuffer_addr = framebuffer.addr().cast::<u8>();
                (*boot_info).framebuffer_width = framebuffer.width();
                (*boot_info).framebuffer_height = framebuffer.height();
                (*boot_info).framebuffer_pitch = framebuffer.pitch();
                (*boot_info).framebuffer_bpp = framebuffer.bpp();
            }
        }
    }

    INITIALIZED.store(true, Ordering::SeqCst);
}

/// Limine 引导器适配器
struct LimineBootloader;

impl LimineBootloader {
    unsafe fn new() -> Self {
        init_boot_info();
        Self
    }

    fn is_supported(&self) -> bool {
        BASE_REVISION.is_supported()
    }

    fn framebuffer_addr(&self) -> *mut u8 {
        unsafe { BOOT_INFO.framebuffer_addr }
    }

    fn framebuffer_width(&self) -> u64 {
        unsafe { BOOT_INFO.framebuffer_width }
    }

    fn framebuffer_height(&self) -> u64 {
        unsafe { BOOT_INFO.framebuffer_height }
    }

    fn framebuffer_pitch(&self) -> u64 {
        unsafe { BOOT_INFO.framebuffer_pitch }
    }

    fn framebuffer_bpp(&self) -> u16 {
        unsafe { BOOT_INFO.framebuffer_bpp }
    }

    fn bootloader_name(&self) -> &'static str {
        use core::ffi::CStr;
        unsafe {
            let boot_info = &raw const BOOT_INFO;
            CStr::from_ptr((*boot_info).bootloader_name.as_ptr() as *const i8)
                .to_str()
                .unwrap_or("Limine")
        }
    }
}

/// Limine 适配器包装结构
/// 实现内核的 Bootloader trait
struct LimineAdapter {
    inner: LimineBootloader,
}

impl LimineAdapter {
    unsafe fn new() -> Self {
        Self {
            inner: unsafe { LimineBootloader::new() },
        }
    }
}

impl Bootloader for LimineAdapter {
    fn get_boot_info(&self) -> BootInfo {
        let framebuffer = if self.inner.framebuffer_addr().is_null() {
            None
        } else {
            Some(KernelFramebufferInfo {
                address: self.inner.framebuffer_addr(),
                width: self.inner.framebuffer_width(),
                height: self.inner.framebuffer_height(),
                pitch: self.inner.framebuffer_pitch(),
                bpp: self.inner.framebuffer_bpp(),
                red_mask_shift: 16,
                red_mask_size: 8,
                green_mask_shift: 8,
                green_mask_size: 8,
                blue_mask_shift: 0,
                blue_mask_size: 8,
            })
        };

        BootInfo {
            framebuffer,
            memory_map: None,
            kernel_load_addr: 0,
            bootloader_name: self.inner.bootloader_name(),
            bootloader_version: None,
        }
    }

    fn is_supported(&self) -> bool {
        self.inner.is_supported()
    }
}

/// 内核入口点
#[unsafe(no_mangle)]
unsafe extern "C" fn kmain() -> ! {
    // 初始化 UART 控制台
    let console = UartConsole::new();
    unsafe { console.init() };

    // 创建 Limine 引导器适配器
    let bootloader = unsafe { LimineAdapter::new() };

    // 检查引导程序支持
    if !bootloader.is_supported() {
        unsafe {
            raw_print("[ERROR] Bootloader not supported!\n");
        }
        hcf();
    }

    // 创建并运行内核
    let mut kernel = Kernel::new(console, &bootloader);
    kernel.run()
}

/// 原始打印（用于错误处理）
unsafe fn raw_print(s: &str) {
    const UART_PORT: u16 = 0x3F8;

    for byte in s.bytes() {
        loop {
            let status: u8;
            unsafe {
                asm!("in al, dx", out("al") status, in("dx") UART_PORT + 5);
            }
            if status & 0x20 != 0 {
                break;
            }
            unsafe {
                asm!("pause");
            }
        }
        unsafe {
            asm!("out dx, al", in("dx") UART_PORT + 0, in("al") byte);
        }
    }
}

/// 停止 CPU
fn hcf() -> ! {
    loop {
        unsafe {
            asm!("hlt");
        }
    }
}

/// Panic handler
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        unsafe {
            asm!("hlt");
        }
    }
}
