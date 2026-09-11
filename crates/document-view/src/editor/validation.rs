//! Opt-in native fault injection. No controls or instrumentation are compiled
//! into normal releases. The harness uses only its private Wayland seat.
use super::*;

actions!(layout_validation, [ArmPlannerPanic, ArmPlannerTimeout]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fault {
    Panic,
    Timeout,
}

pub(super) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new(
            "ctrl-alt-shift-p",
            ArmPlannerPanic,
            Some("LayoutValidation"),
        ),
        KeyBinding::new(
            "ctrl-alt-shift-t",
            ArmPlannerTimeout,
            Some("LayoutValidation"),
        ),
    ]);
}

impl RichDocumentEditor {
    pub(super) fn arm_planner_panic(
        &mut self,
        _: &ArmPlannerPanic,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reflow.native_fault = Some(Fault::Panic);
        self.request_layout_trace(&InspectLayout, window, cx);
        eprintln!("TACHYON_LAYOUT_VALIDATION armed-panic");
    }

    pub(super) fn arm_planner_timeout(
        &mut self,
        _: &ArmPlannerTimeout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reflow.native_fault = Some(Fault::Timeout);
        self.request_layout_trace(&InspectLayout, window, cx);
        eprintln!("TACHYON_LAYOUT_VALIDATION armed-timeout");
    }
}
