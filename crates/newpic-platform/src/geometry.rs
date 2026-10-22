use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DisplayId(String);

impl DisplayId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalPoint {
    pub x: i32,
    pub y: i32,
}

impl PhysicalPoint {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalPoint {
    pub x: f64,
    pub y: f64,
}

impl LogicalPoint {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalSize {
    pub width: u32,
    pub height: u32,
}

impl PhysicalSize {
    pub fn new(width: u32, height: u32) -> Result<Self, GeometryError> {
        if width == 0 || height == 0 {
            return Err(GeometryError::ZeroSize { width, height });
        }
        Ok(Self { width, height })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalRect {
    origin: PhysicalPoint,
    size: PhysicalSize,
}

impl PhysicalRect {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Result<Self, GeometryError> {
        Self::from_origin_size(x, y, PhysicalSize::new(width, height)?)
    }

    pub const fn from_origin_size(
        x: i32,
        y: i32,
        size: PhysicalSize,
    ) -> Result<Self, GeometryError> {
        Ok(Self {
            origin: PhysicalPoint::new(x, y),
            size,
        })
    }

    pub const fn origin(self) -> PhysicalPoint {
        self.origin
    }

    pub const fn size(self) -> PhysicalSize {
        self.size
    }

    pub const fn left(self) -> i64 {
        self.origin.x as i64
    }

    pub const fn top(self) -> i64 {
        self.origin.y as i64
    }

    pub const fn right(self) -> i64 {
        self.left() + self.size.width as i64
    }

    pub const fn bottom(self) -> i64 {
        self.top() + self.size.height as i64
    }

    pub fn contains(self, point: PhysicalPoint) -> bool {
        let x = i64::from(point.x);
        let y = i64::from(point.y);
        x >= self.left() && x < self.right() && y >= self.top() && y < self.bottom()
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        let left = self.left().max(other.left());
        let top = self.top().max(other.top());
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if left >= right || top >= bottom {
            return None;
        }

        let x = i32::try_from(left).ok()?;
        let y = i32::try_from(top).ok()?;
        let width = u32::try_from(right - left).ok()?;
        let height = u32::try_from(bottom - top).ok()?;
        Self::new(x, y, width, height).ok()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScaleFactor(f64);

impl ScaleFactor {
    pub fn new(value: f64) -> Result<Self, GeometryError> {
        if !value.is_finite() || value <= 0.0 {
            return Err(GeometryError::InvalidScaleFactor(value));
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> f64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Display {
    id: DisplayId,
    bounds: PhysicalRect,
    scale_factor: ScaleFactor,
    primary: bool,
}

impl Display {
    pub const fn new(
        id: DisplayId,
        bounds: PhysicalRect,
        scale_factor: ScaleFactor,
        primary: bool,
    ) -> Self {
        Self {
            id,
            bounds,
            scale_factor,
            primary,
        }
    }

    pub const fn id(&self) -> &DisplayId {
        &self.id
    }

    pub const fn bounds(&self) -> PhysicalRect {
        self.bounds
    }

    pub const fn scale_factor(&self) -> ScaleFactor {
        self.scale_factor
    }

    pub const fn is_primary(&self) -> bool {
        self.primary
    }

    /// Converts a display-local logical point into virtual-desktop physical pixels.
    pub fn logical_to_physical(&self, point: LogicalPoint) -> Result<PhysicalPoint, GeometryError> {
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err(GeometryError::NonFiniteLogicalPoint);
        }
        let x = self.bounds.left() as f64 + point.x * self.scale_factor.get();
        let y = self.bounds.top() as f64 + point.y * self.scale_factor.get();
        let rounded_x = x.round();
        let rounded_y = y.round();
        if rounded_x < f64::from(i32::MIN)
            || rounded_x > f64::from(i32::MAX)
            || rounded_y < f64::from(i32::MIN)
            || rounded_y > f64::from(i32::MAX)
        {
            return Err(GeometryError::CoordinateOutOfRange);
        }
        Ok(PhysicalPoint::new(rounded_x as i32, rounded_y as i32))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayIntersection {
    display_id: DisplayId,
    rect: PhysicalRect,
}

impl DisplayIntersection {
    pub const fn display_id(&self) -> &DisplayId {
        &self.display_id
    }

    pub const fn rect(&self) -> PhysicalRect {
        self.rect
    }
}

/// Returns the non-empty pieces of `region` covered by attached displays.
/// Input display order is preserved so platform adapters can use stable order.
pub fn intersections_with_displays(
    region: PhysicalRect,
    displays: &[Display],
) -> Vec<DisplayIntersection> {
    displays
        .iter()
        .filter_map(|display| {
            region
                .intersection(display.bounds)
                .map(|rect| DisplayIntersection {
                    display_id: display.id.clone(),
                    rect,
                })
        })
        .collect()
}

/// F8 regions are valid only while at least one attached display covers a pixel.
pub fn is_region_valid(region: PhysicalRect, displays: &[Display]) -> bool {
    displays
        .iter()
        .any(|display| region.intersection(display.bounds).is_some())
}

/// Builds an F9 rectangle centered at `pointer`, reduced and repositioned so
/// it is wholly contained by the selected display.
pub fn fixed_region_at(
    pointer: PhysicalPoint,
    requested: PhysicalSize,
    display: &Display,
) -> Result<PhysicalRect, GeometryError> {
    let bounds = display.bounds;
    let width = requested.width.min(bounds.size.width);
    let height = requested.height.min(bounds.size.height);
    let centered_x = i64::from(pointer.x) - i64::from(width) / 2;
    let centered_y = i64::from(pointer.y) - i64::from(height) / 2;
    let max_x = bounds.right() - i64::from(width);
    let max_y = bounds.bottom() - i64::from(height);
    let x = centered_x.clamp(bounds.left(), max_x);
    let y = centered_y.clamp(bounds.top(), max_y);

    PhysicalRect::new(
        i32::try_from(x).map_err(|_| GeometryError::CoordinateOutOfRange)?,
        i32::try_from(y).map_err(|_| GeometryError::CoordinateOutOfRange)?,
        width,
        height,
    )
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum GeometryError {
    #[error("physical size must be non-zero, got {width} by {height}")]
    ZeroSize { width: u32, height: u32 },
    #[error("scale factor must be finite and greater than zero, got {0}")]
    InvalidScaleFactor(f64),
    #[error("logical point coordinates must be finite")]
    NonFiniteLogicalPoint,
    #[error("coordinate is outside the supported physical range")]
    CoordinateOutOfRange,
}
