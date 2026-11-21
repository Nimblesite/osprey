//! Native secondary windows share the application state and dispatch to its primary window.
use super::*;

struct Detached {
    host: WeakEntity<HostView>,
    key: String,
}

impl Render for Detached {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(host) = self.host.upgrade() else {
            window.remove_window();
            return div().into_any_element();
        };
        host.update(cx, |view, cx| {
            if view.regions.get(&self.key).is_none() {
                window.remove_window();
                return div().into_any_element();
            }
            div()
                .size_full()
                .bg(view.background(window, cx))
                .text_color(cx.theme().foreground)
                .child(render::region(view, &self.key, window, cx))
                .into_any_element()
        })
    }
}

impl HostView {
    pub(super) fn sync_detached(&mut self, cx: &mut Context<Self>) {
        let key = self.regions.detached();
        if self.detached == key {
            return;
        }
        self.detached.clone_from(&key);
        if let Some(old) = self.detached_handle.take() {
            cx.defer(move |cx| {
                if cx.windows().contains(&old) {
                    let _ = old.update(cx, |_, window, _| window.remove_window());
                }
            });
        }
        let Some(key) = key else { return };
        let Some(primary) = self.primary else { return };
        let host = cx.entity();
        cx.defer(move |cx| {
            let mut opts = options();
            if let Some(bar) = &mut opts.titlebar {
                bar.title = key
                    .splitn(3, ':')
                    .nth(2)
                    .map(|title| title.to_owned().into());
            }
            let owner = host.clone();
            if owner.read(cx).detached.as_ref() != Some(&key) {
                return;
            }
            let result = gpui_kit::open_window(opts, cx, move |_, cx| {
                cx.new(|cx| {
                    cx.observe(&host, |_, _, cx| cx.notify()).detach();
                    cx.on_release(move |child: &mut Detached, cx| {
                        let (owner, key) = (child.host.clone(), child.key.clone());
                        cx.defer(move |cx| {
                            let _ = primary.update(cx, |_, window, cx| {
                                let _ = owner.update(cx, |host, cx| {
                                    if host.detached.as_ref() == Some(&key) {
                                        host.detached_handle = None;
                                        host.send(
                                            Touch::phase(&format!("closed:{key}")),
                                            window,
                                            cx,
                                        );
                                    }
                                });
                            });
                        });
                    })
                    .detach();
                    Detached {
                        host: host.downgrade(),
                        key,
                    }
                })
            });
            owner.update(cx, |view, _| match result {
                Ok((handle, _)) => view.detached_handle = Some(handle),
                Err(error) => view.diagnose(format!("Could not open patch window: {error}")),
            });
        });
    }
}
