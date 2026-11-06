//! Play queues: the list of what plays next.

use serde::Deserialize;

use crate::model::Item;
use crate::ItemId;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayQueue {
    #[serde(rename = "playQueueID")]
    pub id: ItemId,
    /// `playQueueItemID` of the item to start with.
    #[serde(rename = "playQueueSelectedItemID")]
    pub selected_item_id: Option<ItemId>,
    #[serde(default, rename = "playQueueShuffled")]
    pub shuffled: bool,
    #[serde(default, rename = "Metadata")]
    pub items: Vec<Item>,
}

impl PlayQueue {
    /// Id of a queue that only exists in this application.
    pub const LOCAL: ItemId = ItemId::fixed("0");

    /// A queue the server does not know about, for playing offline copies.
    pub fn local(items: Vec<Item>) -> Self {
        Self {
            id: Self::LOCAL,
            selected_item_id: None,
            shuffled: false,
            items,
        }
    }

    /// Whether the server keeps this queue.
    pub fn is_local(&self) -> bool {
        self.id == Self::LOCAL
    }

    /// Position of the selected item in `items`.
    pub fn selected_index(&self) -> Option<usize> {
        let selected = self.selected_item_id?;
        self.items
            .iter()
            .position(|item| item.play_queue_item_id == Some(selected))
    }
}
