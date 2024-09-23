use crate::frame::Frame;

pub trait AsyncHandler {
    type Output;
    type Error;

    async fn handle_frame(&mut self, frame: Frame) -> Result<Self::Output, Self::Error>;
}
