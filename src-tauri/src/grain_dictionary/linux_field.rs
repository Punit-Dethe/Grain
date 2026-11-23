//! Read a single focused, editable AT-SPI text field during an opt-in watch.

use std::time::Duration;

use atspi::object_match::ObjectMatchRule;
use atspi::object_ref::ObjectRefOwned;
use atspi::proxy::{accessible::AccessibleProxy, collection::CollectionProxy, text::TextProxy};
use atspi::zbus::{proxy::CacheProperties, Connection};
use atspi::{AccessibilityConnection, MatchType, Role, SortOrder, State};
use futures_util::{stream, StreamExt};

const DISCOVERY_TIMEOUT: Duration = Duration::from_millis(300);
const READ_TIMEOUT: Duration = Duration::from_millis(200);
const MAX_FIELD_CHARS: i32 = 4096;

pub(super) struct Field {
    accessible: AccessibleProxy<'static>,
    text: TextProxy<'static>,
}

impl Field {
    pub(super) async fn open() -> Option<(Self, String)> {
        // Bound discovery so a slow provider does not hold up dictation delivery.
        tokio::time::timeout(DISCOVERY_TIMEOUT, async {
            let field = Self::discover().await?;
            let before = field.read_focused().await?;
            Some((field, before))
        })
        .await
        .ok()?
    }

    pub(super) async fn selected_text() -> Option<String> {
        tokio::time::timeout(DISCOVERY_TIMEOUT, async {
            let field = Self::discover().await?;
            if !field.is_focused_editable().await? || field.text.get_n_selections().await.ok()? != 1
            {
                return None;
            }
            let (start, end) = field.text.get_selection(0).await.ok()?;
            let chars = end.checked_sub(start)?;
            if start < 0 || !(1..=MAX_FIELD_CHARS).contains(&chars) {
                return None;
            }
            let selection = field.text.get_text(start, end).await.ok()?;
            (selection.len() <= MAX_FIELD_CHARS as usize * 4 && !selection.trim().is_empty())
                .then(|| selection.trim().to_owned())
        })
        .await
        .ok()?
    }

    async fn discover() -> Option<Self> {
        let bus = AccessibilityConnection::new().await.ok()?;
        let desktop = bus.root_accessible_on_registry().await.ok()?;
        let apps = desktop.get_children().await.ok()?;
        if apps.len() > 64 {
            return None;
        }

        let conn = bus.connection();
        let candidates: Vec<_> = stream::iter(apps)
            .map(|app| matches_in_app(conn, app))
            .buffer_unordered(4)
            .flat_map(stream::iter)
            .take(2)
            .collect()
            .await;
        let [target] = candidates.as_slice() else {
            return None;
        };
        let name = target.name()?.clone();
        let path = target.path().clone();
        let accessible = AccessibleProxy::builder(conn)
            .destination(name.clone())
            .ok()?
            .path(path.clone())
            .ok()?
            .cache_properties(CacheProperties::No)
            .build()
            .await
            .ok()?;
        let text = TextProxy::builder(conn)
            .destination(name)
            .ok()?
            .path(path)
            .ok()?
            .cache_properties(CacheProperties::No)
            .build()
            .await
            .ok()?;
        Some(Self { accessible, text })
    }

    pub(super) async fn read(&self) -> Option<String> {
        tokio::time::timeout(READ_TIMEOUT, self.read_focused())
            .await
            .ok()?
    }

    async fn is_focused_editable(&self) -> Option<bool> {
        // Some toolkits provide text for password and terminal controls too.
        if !matches!(
            self.accessible.get_role().await.ok()?,
            Role::Entry | Role::Text
        ) {
            return Some(false);
        }
        let state = self.accessible.get_state().await.ok()?;
        Some(state.contains(State::Focused) && state.contains(State::Editable))
    }

    async fn read_focused(&self) -> Option<String> {
        if !self.is_focused_editable().await? {
            return None;
        }
        let chars = self.text.character_count().await.ok()?;
        if !(0..=MAX_FIELD_CHARS).contains(&chars) {
            return None;
        }
        self.text
            .get_text(0, chars)
            .await
            .ok()
            .filter(|text| text.len() <= MAX_FIELD_CHARS as usize * 4)
    }
}

async fn matches_in_app(conn: &Connection, root: ObjectRefOwned) -> Vec<ObjectRefOwned> {
    async fn query(conn: &Connection, root: ObjectRefOwned) -> Option<Vec<ObjectRefOwned>> {
        let collection = CollectionProxy::builder(conn)
            .destination(root.name()?.clone())
            .ok()?
            .path(root.path().clone())
            .ok()?
            .cache_properties(CacheProperties::No)
            .build()
            .await
            .ok()?;
        let rule = ObjectMatchRule::builder()
            .states([State::Focused, State::Editable], MatchType::All)
            .roles(&[Role::Entry, Role::Text], MatchType::Any)
            .build();
        collection
            .get_matches(rule, SortOrder::Canonical, 2, true)
            .await
            .ok()
    }

    // One provider may be gone or may lack the collection interface.
    query(conn, root).await.unwrap_or_default()
}
