use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{ Paragraph, Widget},
};
use std::any::Any;
use std::fmt::Debug;

use crate::node::NodeState;

pub trait DynamicState: Any + Debug + Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn clone_box(&self) -> Box<dyn DynamicState>;
}

pub trait DynamicStatefulWidget {
    fn render_dynamic(&self, area: Rect, buf: &mut Buffer, state: &mut dyn DynamicState);
}

pub trait DynamicNodeStatefulWidget {
    fn render_dynamic(&self, area: Rect, buf: &mut Buffer, node_state: &NodeState, state: &mut dyn DynamicState);
}

#[derive(Clone, Debug)]
pub struct DefaultWidgetState;

impl DynamicState for DefaultWidgetState {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn clone_box(&self) -> Box<dyn DynamicState> {
        Box::new(self.clone())
    }
}

pub struct NoProviderWidget;

impl DynamicStatefulWidget for NoProviderWidget {
    fn render_dynamic(&self, area: Rect, buf: &mut Buffer, _state: &mut dyn DynamicState) {
        Paragraph::new("No Provider").render(area, buf);
    }
}