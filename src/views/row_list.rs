//! Pages that are one virtualised list of different kinds of rows: a header
//! with details first, then episodes or tracks. Only the rows in view exist
//! as widgets, however long the list is.

use gtk::prelude::*;
use gtk::{gio, glib};

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

    pub fn activatable(widget: &impl IsA<gtk::Widget>) -> Self {
        Self {
            widget: widget.clone().upcast(),
            activatable: true,
        }
    }
}

/// Connects `list` to a new store of rows of type `R`. `build` makes the
/// widget of a row when it scrolls into view, `activate` handles a click on
/// an activatable row.
pub fn setup<R: 'static>(
    list: &gtk::ListView,
    build: impl Fn(&R) -> RowWidget + 'static,
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
        let built = build(&row.borrow::<R>());
        list_item.set_activatable(built.activatable);
        list_item.set_selectable(false);
        list_item.set_child(Some(&built.widget));
    });
    factory.connect_unbind(|_, object| {
        if let Some(list_item) = object.downcast_ref::<gtk::ListItem>() {
            // Releases the widget; a header widget is reused when its row
            // scrolls back into view.
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
