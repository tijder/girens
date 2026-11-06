//! A scrolling row, or in grid mode a grid, of cover tiles.

use std::cell::{Cell, RefCell};

use adw::subclass::prelude::*;
use girens_model::model::Item;
use gtk::prelude::*;
use gtk::{gio, glib, CompositeTemplate};

use super::CoverBox;

const DEFAULT_COVER_WIDTH: i32 = 200;

/// What a tile of the list stands for.
enum Entry {
    Item(Box<Item>),
    /// An empty tile that keeps the place of an item that is being loaded.
    Placeholder {
        image_height: i32,
    },
}

fn entries(items: Vec<Item>) -> Vec<glib::BoxedAnyObject> {
    items
        .into_iter()
        .map(|item| glib::BoxedAnyObject::new(Entry::Item(Box::new(item))))
        .collect()
}

mod imp {
    use super::*;

    #[derive(CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/list.ui")]
    pub struct List {
        #[template_child]
        pub _scrolled_window: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub _grid_view: TemplateChild<gtk::GridView>,
        #[template_child]
        pub _empty_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub _previous_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub _next_button: TemplateChild<gtk::Button>,
        pub grid_mode: Cell<bool>,
        /// Whether the tiles are placeholders, see `set_placeholders`.
        pub placeholders: Cell<bool>,
        pub store: gio::ListStore,
        pub cover_width: Cell<i32>,
        pub in_show_view: Cell<bool>,
        pub on_end_reached: RefCell<Option<Box<dyn Fn()>>>,
    }

    impl Default for List {
        fn default() -> Self {
            Self {
                _scrolled_window: TemplateChild::default(),
                _grid_view: TemplateChild::default(),
                _empty_page: TemplateChild::default(),
                _previous_button: TemplateChild::default(),
                _next_button: TemplateChild::default(),
                grid_mode: Cell::new(false),
                placeholders: Cell::new(false),
                store: gio::ListStore::new::<glib::BoxedAnyObject>(),
                cover_width: Cell::new(DEFAULT_COVER_WIDTH),
                in_show_view: Cell::new(false),
                on_end_reached: RefCell::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for List {
        const NAME: &'static str = "list";
        type Type = super::List;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for List {
        fn constructed(&self) {
            self.parent_constructed();
            let list = self.obj();

            let factory = gtk::SignalListItemFactory::new();
            factory.connect_setup(glib::clone!(
                #[weak]
                list,
                move |_, object| {
                    let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
                        return;
                    };
                    list_item.set_activatable(false);
                    list_item.set_child(Some(&CoverBox::new(list.imp().in_show_view.get())));
                }
            ));
            factory.connect_bind(glib::clone!(
                #[weak]
                list,
                move |_, object| {
                    let Some((cover, entry)) = cover_and_entry(object) else {
                        return;
                    };
                    let width = list.imp().cover_width.get();
                    let entry = entry.borrow::<Entry>();
                    match &*entry {
                        Entry::Item(item) => cover.set_item(item, width),
                        Entry::Placeholder { image_height } => {
                            cover.set_placeholder(width, *image_height)
                        }
                    }
                }
            ));
            factory.connect_unbind(|_, object| {
                if let Some((cover, _)) = cover_and_entry(object) {
                    cover.unbind();
                }
            });

            self._grid_view.set_factory(Some(&factory));
            self._grid_view
                .set_model(Some(&gtk::NoSelection::new(Some(self.store.clone()))));

            // Buttons to page through a row; they show when there is more in
            // their direction.
            let adjustment = self._scrolled_window.hadjustment();
            for signal in ["changed", "value-changed"] {
                adjustment.connect_local(
                    signal,
                    false,
                    glib::clone!(
                        #[weak]
                        list,
                        #[upgrade_or]
                        None,
                        move |_| {
                            list.update_paging_buttons();
                            None
                        }
                    ),
                );
            }
            for (button, direction) in [(&self._previous_button, -1.0), (&self._next_button, 1.0)] {
                button.connect_clicked(glib::clone!(
                    #[weak]
                    list,
                    move |_| {
                        let adjustment = list.imp()._scrolled_window.hadjustment();
                        let step = adjustment.page_size() * 0.8 * direction;
                        adjustment.set_value(adjustment.value() + step);
                    }
                ));
            }

            self._scrolled_window.connect_edge_reached(glib::clone!(
                #[weak]
                list,
                move |_, position| {
                    if position != gtk::PositionType::Bottom {
                        return;
                    }
                    if let Some(callback) = list.imp().on_end_reached.borrow().as_ref() {
                        callback();
                    }
                }
            ));
        }
    }

    impl WidgetImpl for List {}
    impl BinImpl for List {}

    fn cover_and_entry(object: &glib::Object) -> Option<(CoverBox, glib::BoxedAnyObject)> {
        let list_item = object.downcast_ref::<gtk::ListItem>()?;
        Some((
            list_item.child().and_downcast::<CoverBox>()?,
            list_item.item().and_downcast::<glib::BoxedAnyObject>()?,
        ))
    }
}

glib::wrapper! {
    pub struct List(ObjectSubclass<imp::List>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for List {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl List {
    pub fn set_items(&self, items: Vec<Item>) {
        let imp = self.imp();
        imp.placeholders.set(false);
        imp.store.splice(0, imp.store.n_items(), &entries(items));
    }

    pub fn append(&self, items: Vec<Item>) {
        self.imp().store.extend_from_slice(&entries(items));
    }

    /// Shows `count` empty tiles with an image of `image_height`, so that the
    /// list already takes its place while its items are loaded.
    pub fn set_placeholders(&self, count: usize, image_height: i32) {
        let imp = self.imp();
        let objects: Vec<glib::BoxedAnyObject> = (0..count)
            .map(|_| glib::BoxedAnyObject::new(Entry::Placeholder { image_height }))
            .collect();
        imp.placeholders.set(true);
        imp._empty_page.set_visible(false);
        imp.store.splice(0, imp.store.n_items(), &objects);
        self.update_paging_buttons();
    }

    pub fn clear(&self) {
        self.imp().placeholders.set(false);
        self.imp().store.remove_all();
    }

    /// Says `message` where the tiles would be; `None` removes it.
    pub fn set_empty_message(&self, message: Option<&str>) {
        let imp = self.imp();
        imp._empty_page.set_visible(message.is_some());
        if let Some(message) = message {
            imp._empty_page.set_title(message);
        }
    }

    fn update_paging_buttons(&self) {
        let imp = self.imp();
        let adjustment = imp._scrolled_window.hadjustment();
        let scrollable = !imp.grid_mode.get()
            && !imp.placeholders.get()
            && adjustment.upper() > adjustment.page_size() + 1.0;
        imp._previous_button
            .set_visible(scrollable && adjustment.value() > adjustment.lower() + 1.0);
        imp._next_button.set_visible(
            scrollable && adjustment.value() + adjustment.page_size() < adjustment.upper() - 1.0,
        );
    }

    pub fn len(&self) -> u32 {
        self.imp().store.n_items()
    }

    /// Changes the width of the tiles, for narrow windows.
    pub fn set_cover_width(&self, width: i32) {
        let imp = self.imp();
        if imp.cover_width.replace(width) != width {
            // Rebinds the visible tiles so that they take the new size.
            let count = imp.store.n_items();
            imp.store.items_changed(0, count, count);
        }
    }

    /// Replaces the item with the same rating key, if it is in the list.
    pub fn update_item(&self, item: &Item) {
        let store = &self.imp().store;
        for position in 0..store.n_items() {
            let found = store
                .item(position)
                .and_downcast::<glib::BoxedAnyObject>()
                .is_some_and(|object| {
                    matches!(&*object.borrow::<Entry>(),
                        Entry::Item(shown) if shown.rating_key == item.rating_key)
                });
            if found {
                store.splice(position, 1, &entries(vec![item.clone()]));
            }
        }
    }

    /// Switches from one horizontal row to a vertically scrolling grid.
    pub fn set_grid_mode(&self) {
        let imp = self.imp();
        imp.grid_mode.set(true);
        imp._previous_button.set_visible(false);
        imp._next_button.set_visible(false);
        imp._grid_view.set_orientation(gtk::Orientation::Vertical);
        imp._grid_view.set_max_columns(10);
        imp._scrolled_window
            .set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        imp._scrolled_window.set_vexpand(true);
    }

    /// Tiles inside a show page present episodes differently.
    pub fn set_in_show_view(&self, in_show_view: bool) {
        self.imp().in_show_view.set(in_show_view);
    }

    /// Called when the user scrolls to the end of a grid.
    pub fn connect_end_reached(&self, callback: impl Fn() + 'static) {
        self.imp().on_end_reached.replace(Some(Box::new(callback)));
    }
}
