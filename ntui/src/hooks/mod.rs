#[allow(dead_code)]
pub(crate) enum HookSlot {} // variants added by hook tasks

/// Handle passed to every component render. Hook identity = call order.
pub struct Hooks<'a> {
    #[allow(dead_code)]
    pub(crate) slots: &'a mut Vec<HookSlot>,
    #[allow(dead_code)]
    pub(crate) cursor: usize,
    #[allow(dead_code)]
    pub(crate) component_name: &'static str,
}

impl<'a> Hooks<'a> {
    #[allow(dead_code)]
    pub(crate) fn new(slots: &'a mut Vec<HookSlot>, component_name: &'static str) -> Self {
        Hooks {
            slots,
            cursor: 0,
            component_name,
        }
    }
}
