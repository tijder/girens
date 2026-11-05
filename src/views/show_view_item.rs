//! One episode in the list of a show page.

use std::cell::RefCell;

use adw::subclass::prelude::*;
use girens_plex::model::Item;
use gtk::prelude::*;
use gtk::{glib, CompositeTemplate};

use crate::session::Session;
use crate::{images, util};

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/nl/g4d/Girens/show_view_item.ui")]
    pub struct ShowViewItem {
        #[template_child]
        pub _cover_image: TemplateChild<gtk::Picture>,
        #[template_child]
        pub _title_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _release_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _description_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub _progress_bar: TemplateChild<gtk::ProgressBar>,
        #[template_child]
        pub _watched_image: TemplateChild<gtk::Image>,
        pub image_task: RefCell<Option<glib::JoinHandle<()>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ShowViewItem {
        const NAME: &'static str = "show_view_item";
        type Type = super::ShowViewItem;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ShowViewItem {
        fn dispose(&self) {
            if let Some(task) = self.image_task.take() {
                task.abort();
            }
        }
    }

    impl WidgetImpl for ShowViewItem {}
    impl BoxImpl for ShowViewItem {}
}

glib::wrapper! {
    pub struct ShowViewItem(ObjectSubclass<imp::ShowViewItem>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl ShowViewItem {
    pub fn new(episode: &Item) -> Self {
        let row: Self = glib::Object::new();
        let imp = row.imp();

        let title = match episode.index {
            Some(index) => format!("{index} - {}", episode.title),
            None => episode.title.clone(),
        };
        imp._title_label.set_text(&title);
        let released = episode
            .originally_available_at
            .as_deref()
            .and_then(util::format_date)
            .unwrap_or_default();
        imp._release_label.set_text(&released);
        imp._description_label
            .set_text(episode.summary.as_deref().unwrap_or_default());

        match (episode.view_offset, episode.duration) {
            (offset, Some(duration)) if offset != 0 && duration > 0 => {
                imp._progress_bar
                    .set_fraction((offset as f64 / duration as f64).min(1.0));
                imp._progress_bar.set_visible(true);
            }
            _ => imp._progress_bar.set_visible(false),
        }
        imp._watched_image.set_visible(!episode.is_watched());

        if let (Some(thumb), Some(server)) = (episode.thumb.clone(), Session::current().server()) {
            let key = episode.rating_key;
            let task = glib::spawn_future_local(glib::clone!(
                #[weak]
                row,
                async move {
                    if let Some(texture) = images::cover(&server, key, &thumb).await {
                        row.imp()._cover_image.set_paintable(Some(&texture));
                    }
                }
            ));
            imp.image_task.replace(Some(task));
        }
        row
    }
}
