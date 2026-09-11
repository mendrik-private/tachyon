//! Application chrome over GPUI's native window operations and toolkit buttons.
use document_view::ButtonAccessibilityExt as _;
use gpui::{
    AnyElement, App, ClickEvent, Context, Decorations, DismissEvent, ElementId, Entity,
    Focusable as _, InteractiveElement as _, IntoElement, MouseButton, ParentElement, Render,
    RenderOnce, StatefulInteractiveElement as _, Styled as _, TitlebarOptions, Window,
    WindowControlArea, div, prelude::FluentBuilder as _, px, rgb,
};
use gpui_component::{
    ActiveTheme as _, FocusableExt as _, IconName, InteractiveElementExt as _, Sizable as _,
    button::{Button, ButtonCustomVariant, ButtonVariants as _},
    menu::PopupMenu,
    popover::Popover,
};
use std::rc::Rc;

const FOREGROUND: u32 = 0xe8ebed;

fn background(cx: &App) -> u32 {
    if cx.theme().is_dark() {
        0x172027
    } else {
        0x272b2e
    }
}

/// Fixed geometry across normal, hover, pressed, disabled and keyboard focus.
pub fn button(id: impl Into<ElementId>, cx: &App) -> Button {
    Button::new(id)
        .custom(
            ButtonCustomVariant::new(cx)
                .color(rgb(0x343a3f).into())
                .foreground(rgb(FOREGROUND).into())
                .hover(rgb(0x485057).into())
                .active(rgb(0x5b656d).into()),
        )
        .small()
        .w(px(28.))
        .h(px(28.))
        .flex_shrink_0()
        .rounded(px(14.))
        .border_1()
        .border_color(rgb(0x343a3f))
        .focus_ring(false)
        .focus_visible(|style| style.border_color(rgb(FOREGROUND)))
        .on_mouse_down(MouseButton::Left, |_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
        })
}

#[derive(Default)]
struct MenuState {
    open: bool,
    menu: Option<Entity<PopupMenu>>,
}
impl Render for MenuState {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// Title-bar buttons consume pointer-down to prevent window dragging. Open the
/// controlled popup from the button's Click for both pointer and keyboard input.
pub fn menu(
    trigger: Button,
    window: &mut Window,
    cx: &mut App,
    builder: impl Fn(PopupMenu, &mut Window, &mut App) -> PopupMenu + 'static,
) -> Popover {
    let state = window.use_keyed_state("title-menu-state", cx, |_, _| MenuState::default());
    let open = state.read(cx).open;
    let keyboard_state = state.clone();
    let changed_state = state.clone();
    let builder = Rc::new(builder);
    Popover::new("title-menu-popover")
        .appearance(false)
        .overlay_closable(false)
        .open(open)
        .trigger(trigger.on_click(move |_, _, cx| {
            keyboard_state.update(cx, |state, cx| {
                state.open = !state.open;
                cx.notify();
            });
        }))
        .on_open_change(move |open, _, cx| {
            changed_state.update(cx, |state, cx| {
                state.open = *open;
                if !open {
                    state.menu = None;
                }
                cx.notify();
            });
        })
        .content(move |_, window, cx| {
            if let Some(menu) = state.read(cx).menu.clone() {
                return menu;
            }
            let builder = builder.clone();
            let menu = PopupMenu::build(window, cx, move |menu, window, cx| {
                builder(menu, window, cx)
            });
            menu.focus_handle(cx).focus(window, cx);
            state.update(cx, |state, _| state.menu = Some(menu.clone()));
            let popover = cx.entity();
            window
                .subscribe(&menu, cx, {
                    let state = state.clone();
                    move |_, _: &DismissEvent, window, cx| {
                        popover.update(cx, |popover, cx| popover.dismiss(window, cx));
                        state.update(cx, |state, cx| {
                            state.open = false;
                            state.menu = None;
                            cx.notify();
                        });
                    }
                })
                .detach();
            menu
        })
}

type CloseHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct TitleBar {
    children: Vec<AnyElement>,
    close: CloseHandler,
}

impl TitleBar {
    pub fn new(close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        Self {
            children: Vec::new(),
            close: Box::new(close),
        }
    }

    pub fn title_bar_options() -> TitlebarOptions {
        gpui_component::TitleBar::title_bar_options()
    }
}

impl ParentElement for TitleBar {
    fn extend(&mut self, children: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(children);
    }
}

struct DragState(bool);
impl Render for DragState {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl RenderOnce for TitleBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let client_decorated = matches!(window.window_decorations(), Decorations::Client { .. });
        let state = window.use_state(cx, |_, _| DragState(false));
        let supported = window.window_controls();
        let maximized = window.is_maximized();
        let mut controls = div()
            .id("window-controls")
            .flex()
            .items_center()
            .gap(px(4.))
            .flex_shrink_0()
            .on_click(|_, _, cx| cx.stop_propagation());
        if cfg!(target_os = "linux") && client_decorated {
            if supported.minimize {
                controls = controls.child(
                    button("window-minimize", cx)
                        .icon(IconName::WindowMinimize)
                        .accessible_name("Minimize window")
                        .tooltip("Minimize window")
                        .on_click(|_, window, _| window.minimize_window()),
                );
            }
            if supported.maximize {
                let label = if maximized {
                    "Restore window"
                } else {
                    "Maximize window"
                };
                controls = controls.child(
                    button("window-maximize", cx)
                        .icon(if maximized {
                            IconName::WindowRestore
                        } else {
                            IconName::WindowMaximize
                        })
                        .accessible_name(label)
                        .tooltip(label)
                        .on_click(|_, window, _| window.zoom_window()),
                );
            }
            let close = self.close;
            controls = controls.child(
                button("window-close", cx)
                    .icon(IconName::WindowClose)
                    .accessible_name("Close window")
                    .tooltip("Close window")
                    .on_click(move |event, window, cx| {
                        close(event, window, cx);
                    }),
            );
        }
        div()
            .id("title-bar")
            .flex()
            .items_center()
            .h(px(34.))
            .flex_shrink_0()
            .pl(px(if cfg!(target_os = "macos") { 80. } else { 8. }))
            .pr(px(6.))
            .bg(rgb(background(cx)))
            .text_color(rgb(FOREGROUND))
            .border_b_1()
            .border_color(rgb(0x424b52))
            .on_mouse_down_out(window.listener_for(&state, |state, _, _, _| state.0 = false))
            .on_mouse_down(
                MouseButton::Left,
                window.listener_for(&state, |state, _, _, _| state.0 = true),
            )
            .on_mouse_up(
                MouseButton::Left,
                window.listener_for(&state, |state, _, _, _| state.0 = false),
            )
            .on_mouse_move(window.listener_for(&state, |state, _, window, _| {
                if state.0 {
                    state.0 = false;
                    window.start_window_move();
                }
            }))
            .on_double_click(|_, window, _| {
                if cfg!(target_os = "macos") {
                    window.titlebar_double_click();
                } else if window.window_controls().maximize {
                    window.zoom_window();
                }
            })
            .when(client_decorated, |bar| {
                bar.on_mouse_down(MouseButton::Right, |event, window, _| {
                    window.show_window_menu(event.position);
                })
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .window_control_area(WindowControlArea::Drag)
                    .children(self.children),
            )
            .child(controls)
    }
}
