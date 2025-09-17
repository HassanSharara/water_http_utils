

#[derive(Debug)]
pub (crate) enum CreatingRequestSteps {
    FirstLine,
    Headers,
}

impl CreatingRequestSteps {

    pub (crate) fn init()->Self{
        Self::FirstLine
    }
}



