//! Pages that are one virtualised list of different kinds of rows: a header
//! with details first, then episodes or tracks. Only the rows in view exist
//! as widgets, however long the list is.

use gtk::prelude::*;
use gtk::{gio, glib};

/// Name given to widgets that exist once and move between rows, such as a
/// page header; they are released when their row scrolls out of view.
const SHARED: &str = "girens-shared-row";

/// What a row shows and whether clicking it does something.
pub struct RowWidget {
    pub widget: gtk::Widget,
    pub activatable: bool,
}

impl RowWidget {
    pub fn inert(widget: &impl IsA<gtk::Widget>) -> Self {
        Self {
            widget: widget.clone().upcast(),
            activatable: false,
        }
    }

    /// A widget that exists only once, like the header of a page.
    pub fn shared(widget: &impl IsA<gtk::Widget>) -> Self {
        widget.set_widget_name(SHARED);
        Self::inert(widget)
    }

    pub fn activatable(widget: &impl IsA<gtk::Widget>) -> Self {
        Self {
            widget: widget.clone().upcast(),
            activatable: true,
        }
    }
}

/// Connects `list` to a new store of rows of type `R`. `build` makes the
/// widget of a row when it scrolls into view; it is handed the widget the
/// list item showed before, to reuse when it is of the right kind.
/// `activate` handles a click on an activatable row.
pub fn setup<R: 'static>(
    list: &gtk::ListView,
    build: impl Fn(&R, Option<gtk::Widget>) -> RowWidget + 'static,
    activate: impl Fn(&R) + 'static,
) -> gio::ListStore {
    let store = gio::ListStore::new::<glib::BoxedAnyObject>();

    let factory = gtk::SignalListItemFactory::new();
    factory.connect_bind(move |_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(row) = list_item.item().and_downcast::<glib::BoxedAnyObject>() else {
            return;
        };
        let previous = list_item.child();
        let built = build(&row.borrow::<R>(), previous.clone());
        list_item.set_activatable(built.activatable);
        list_item.set_selectable(false);
        if previous.as_ref() != Some(&built.widget) {
            list_item.set_child(Some(&built.widget));
        }
    });
    factory.connect_unbind(|_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        // Ordinary rows keep their widget for the next item; a shared widget
        // has to be free to appear in another list item.
        let shared = list_item
            .child()
            .is_some_and(|child| child.widget_name() == SHARED);
        if shared {
            list_item.set_child(gtk::Widget::NONE);
        }
    });
    list.set_factory(Some(&factory));
    list.set_model(Some(&gtk::NoSelection::new(Some(store.clone()))));

    list.connect_activate(glib::clone!(
        #[weak]
        store,
        move |_, position| {
            if let Some(row) = store.item(position).and_downcast::<glib::BoxedAnyObject>() {
                activate(&row.borrow::<R>());
            }
        }
    ));
    store
}

/// Replaces everything after the first `keep` rows of `store`.
pub fn replace_from<R: 'static>(store: &gio::ListStore, keep: u32, rows: Vec<R>) {
    let objects: Vec<glib::BoxedAnyObject> =
        rows.into_iter().map(glib::BoxedAnyObject::new).collect();
    let keep = keep.min(store.n_items());
    store.splice(keep, store.n_items() - keep, &objects);
}

/// A row that shows that more is being loaded.
pub fn loading_row() -> RowWidget {
    let spinner = adw::Spinner::builder()
        .halign(gtk::Align::Center)
        .margin_top(24)
        .margin_bottom(24)
        .width_request(32)
        .height_request(32)
        .build();
    RowWidget::inert(&spinner)
}
