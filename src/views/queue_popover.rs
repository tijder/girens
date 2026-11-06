//! The play queue: a list of what is coming, to jump to another entry.

use adw::subclass::prelude::*;
use girens_model::model::{Item, ItemKind};
use gtk::prelude::*;
use gtk::{glib, pango};

use crate::player::{Event, Player};

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct QueuePopover {
        pub titles: gtk::StringList,
        pub list: gtk::ListView,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for QueuePopover {
        const NAME: &'static str = "GirensQueuePopover";
        type Type = super::QueuePopover;
        type ParentType = gtk::Popover;
    }

    impl ObjectImpl for QueuePopover {
        fn constructed(&self) {
            self.parent_constructed();
            // A list view only creates rows for what is visible, however long
            // the queue is.
            self.list
                .set_model(Some(&gtk::NoSelection::new(Some(self.titles.clone()))));
            self.list.set_single_click_activate(true);
            self.list.add_css_class("navigation-sidebar");
            let scrolled = gtk::ScrolledWindow::builder()
                .hscrollbar_policy(gtk::PolicyType::Never)
                .propagate_natural_height(true)
                .max_content_height(400)
                .width_request(300)
                .child(&self.list)
                .build();
            self.obj().set_child(Some(&scrolled));
        }
    }

    impl WidgetImpl for QueuePopover {}
    impl PopoverImpl for QueuePopover {}
}

glib::wrapper! {
    pub struct QueuePopover(ObjectSubclass<imp::QueuePopover>)
        @extends gtk::Popover, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Native,
                    gtk::ShortcutManager;
}

fn label(item: &Item) -> String {
    match item.kind {
        ItemKind::Episode => match item.season_episode() {
            Some(number) => format!("{number} - {}", item.title),
            None => item.title.clone(),
        },
        _ => item.title.clone(),
    }
}

impl QueuePopover {
    pub fn new(player: &Player) -> Self {
        let popover: Self = glib::Object::new();
        let imp = popover.imp();

        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(|_, object| {
            if let Some(list_item) = object.downcast_ref::<gtk::ListItem>() {
                let text = gtk::Label::builder()
                    .xalign(0.0)
                    .ellipsize(pango::EllipsizeMode::End)
                    .build();
                list_item.set_child(Some(&text));
            }
        });
        factory.connect_bind(glib::clone!(
            #[strong]
            player,
            move |_, object| {
                let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
                    return;
                };
                let (Some(text), Some(title)) = (
                    list_item.child().and_downcast::<gtk::Label>(),
                    list_item.item().and_downcast::<gtk::StringObject>(),
                ) else {
                    return;
                };
                text.set_label(&title.string());
                // The entry that is playing stands out.
                if list_item.position() as usize == player.queue_index() {
                    text.add_css_class("heading");
                } else {
                    text.remove_css_class("heading");
                }
            }
        ));
        imp.list.set_factory(Some(&factory));

        imp.list.connect_activate(glib::clone!(
            #[weak]
            popover,
            #[strong]
            player,
            move |_, position| {
                player.play_index(position as usize);
                popover.popdown();
            }
        ));

        // The list is only filled when it is about to be looked at.
        popover.connect_show(glib::clone!(
            #[strong]
            player,
            move |popover| popover.fill(&player)
        ));
        player.connect(glib::clone!(
            #[weak]
            popover,
            #[strong]
            player,
            move |event| {
                if matches!(event, Event::QueueChanged | Event::Started { .. })
                    && popover.is_visible()
                {
                    popover.fill(&player);
                }
            }
        ));
        popover
    }

    fn fill(&self, player: &Player) {
        let titles: Vec<String> = player.queue_items().iter().map(label).collect();
        let titles: Vec<&str> = titles.iter().map(String::as_str).collect();
        let model = &self.imp().titles;
        model.splice(0, model.n_items(), &titles);
    }
}
