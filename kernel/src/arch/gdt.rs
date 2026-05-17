use x86_64::{
    instructions::tables::load_tss,
    registers::segmentation::{Segment, CS, DS, ES, SS},
    structures::{
        gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector},
        tss::TaskStateSegment,
    },
    VirtAddr,
};

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

static mut TSS: TaskStateSegment = TaskStateSegment::new();
const DF_STACK_SIZE: usize = 4096 * 5;
static mut DF_STACK: [u8; DF_STACK_SIZE] = [0; DF_STACK_SIZE];

static mut GDT: GlobalDescriptorTable = GlobalDescriptorTable::new();
static mut CODE_SELECTOR: SegmentSelector = SegmentSelector(0);
static mut DATA_SELECTOR: SegmentSelector = SegmentSelector(0);
static mut TSS_SELECTOR: SegmentSelector = SegmentSelector(0);
static mut USER_CODE_SELECTOR: SegmentSelector = SegmentSelector(0);
static mut USER_DATA_SELECTOR: SegmentSelector = SegmentSelector(0);

#[allow(static_mut_refs)]
pub fn init(kernel_stack_top: u64) {
    unsafe {
        // set up TSS
        TSS = TaskStateSegment::new();
        TSS.privilege_stack_table[0] = VirtAddr::new(kernel_stack_top);
        TSS.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            let stack_start = VirtAddr::from_ptr(&raw const DF_STACK);
            let stack_end = stack_start + DF_STACK_SIZE as u64;
            stack_end
        };

        // build GDT: null, code, data, TSS
        GDT = GlobalDescriptorTable::new();
        CODE_SELECTOR = GDT.append(Descriptor::kernel_code_segment());
        DATA_SELECTOR = GDT.append(Descriptor::kernel_data_segment());
        USER_CODE_SELECTOR = GDT.append(Descriptor::user_code_segment());
        USER_DATA_SELECTOR = GDT.append(Descriptor::user_data_segment());
        TSS_SELECTOR = GDT.append(Descriptor::tss_segment(&TSS));

        GDT.load();

        CS::set_reg(CODE_SELECTOR);
        DS::set_reg(DATA_SELECTOR);
        ES::set_reg(DATA_SELECTOR);
        SS::set_reg(DATA_SELECTOR);

        load_tss(TSS_SELECTOR);
    }
}

pub fn user_code_selector() -> SegmentSelector {
    unsafe { USER_CODE_SELECTOR }
}
pub fn user_data_selector() -> SegmentSelector {
    unsafe { USER_DATA_SELECTOR }
}
pub fn kernel_code_selector() -> SegmentSelector {
    unsafe { CODE_SELECTOR }
}
pub fn kernel_data_selector() -> SegmentSelector {
    unsafe { DATA_SELECTOR }
}

pub fn set_rsp0(rsp0: u64) {
    unsafe {
        TSS.privilege_stack_table[0] = VirtAddr::new(rsp0);
    }
}
