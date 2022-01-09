use crate::models::display::concepts::color::Color;
use crate::models::display::concepts::stave_spaces::{StavePoint, StaveSpaces, STAVE_SPACES_ZERO};
use crate::models::display::concepts::stroke::StrokeStyle;
use crate::models::display::engraving::engravable::line::EngravedLine;
use crate::models::display::engraving::engravable::Engravable;
use crate::models::display::engraving::region::system::EngravedSystem;
use crate::models::display::grid::horizontal::{
    HorizontalGridLine, HorizontalGridLineConstraint, HorizontalGridLineIndex,
    HorizontalGridLineType,
};
use crate::models::display::grid::vertical::{
    VerticalGridLine, VerticalGridLineConstraint, VerticalGridLineIndex, VerticalGridLineType,
};
use crate::models::display::layout::block::{Block, BlockIndex};
use crate::models::display::layout::block::{BlockConstraint, BlockEnum, BlockLayer};
use crate::models::music::concepts::ticks::Ticks;
use crate::protos::display::stylesheet::SystemJustification;
use cassowary::strength::{REQUIRED, STRONG, WEAK};
use cassowary::WeightedRelation::{EQ, GE, LE};
use cassowary::{AddConstraintError, AddEditVariableError, Solver, SuggestValueError, Variable};
use iset::IntervalMap;
use itertools::izip;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// A two-dimensional layout of Blocks on a System, defined by flat vertical
/// and horizontal grid lines. These grid lines have no width or height themselves;
/// they simply express a single (initially undefined) coordinate on their plane
/// of alignment. Blocks are positioned in the grid by constraining their edges
/// to align with various grid lines. Once all Blocks have been positioned on the
/// grid, computation of the final coordinate positions of all grid lines can be expressed
/// as a set of linear constraints, suitable for feeding into a linear constraint solver.
#[derive(Debug)]
pub struct LayoutSystem {
    index_in_movement: u32,
    start_ticks: Ticks,
    end_ticks: Ticks,
    justification: SystemJustification,
    target_system_width: StaveSpaces,
    horizontal_grid_lines: Vec<HorizontalGridLine>,
    vertical_grid_lines: Vec<VerticalGridLine>,
    top_edge: HorizontalGridLineIndex,
    leading_edge: VerticalGridLineIndex,
    blocks: Vec<BlockEnum>,
    debug_do_draw_horizontal_grid_lines: bool,
    debug_do_draw_vertical_grid_lines: bool,
    debug_do_show_rhythmic_spacing: bool,
    debug_do_draw_block_outlines: bool,
}

impl LayoutSystem {
    /// Creates a new LayoutSystem from the given arguments.
    ///
    /// To build a LayoutSystem from a CastSystem, use a LayoutSystemBuilder.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        index_in_movement: u32,
        start_ticks: Ticks,
        end_ticks: Ticks,
        justification: SystemJustification,
        target_system_width: StaveSpaces,
        horizontal_grid_lines: Vec<HorizontalGridLine>,
        vertical_grid_lines: Vec<VerticalGridLine>,
        top_edge: HorizontalGridLineIndex,
        leading_edge: VerticalGridLineIndex,
        blocks: Vec<BlockEnum>,
        debug_do_draw_horizontal_grid_lines: bool,
        debug_do_draw_vertical_grid_lines: bool,
        debug_do_show_rhythmic_spacing: bool,
        debug_do_draw_block_outlines: bool,
    ) -> Self {
        LayoutSystem {
            index_in_movement,
            start_ticks,
            end_ticks,
            justification,
            target_system_width,
            horizontal_grid_lines,
            vertical_grid_lines,
            top_edge,
            leading_edge,
            blocks,
            debug_do_draw_horizontal_grid_lines,
            debug_do_draw_vertical_grid_lines,
            debug_do_show_rhythmic_spacing,
            debug_do_draw_block_outlines,
        }
    }

    /// Returns the requested system alignment or justification setting for this LayoutSystem.
    #[inline]
    pub fn get_justification(&self) -> SystemJustification {
        self.justification
    }

    /// Returns the target system width of the CastSystem from which this LayoutSystem was generated.
    #[inline]
    pub fn get_target_system_width(&self) -> StaveSpaces {
        self.target_system_width
    }

    /// Returns a slice of all the HorizontalGridLines on this LayoutSystemGrid.
    #[inline]
    pub fn get_horizontal_grid_lines(&self) -> &[HorizontalGridLine] {
        self.horizontal_grid_lines.as_slice()
    }

    /// Returns a slice of all the VerticalGridLines on this LayoutSystemGrid.
    #[inline]
    pub fn get_vertical_grid_lines(&self) -> &[VerticalGridLine] {
        self.vertical_grid_lines.as_slice()
    }

    /// Returns the index of the HorizontalGridLine that marks the top edge of this system.
    /// This top edge, together with the system's leading edge, defines the origin point
    /// of the system. All other grid line and Block constraints will ultimately be resolved
    /// relative to the system's origin.
    #[inline]
    pub fn get_top_edge(&self) -> HorizontalGridLineIndex {
        self.top_edge
    }

    /// Returns the index of the VerticalGridLine that marks the leading edge of this system.
    /// This leading edge, together with the system's top edge, defines the origin point
    /// of the system. All other grid line and Block constraints will ultimately be resolved
    /// relative to the system's origin.
    #[inline]
    pub fn get_leading_edge(&self) -> VerticalGridLineIndex {
        self.leading_edge
    }

    /// Returns a slice of all the Blocks on this LayoutSystemGrid.
    #[inline]
    pub fn get_blocks(&self) -> &[BlockEnum] {
        self.blocks.as_slice()
    }

    /// Generates a final positioned EngravedSystem from this LayoutSystem
    /// by expressing all constraints on grid lines and Blocks in the layout
    /// as a linear constraint system. The output from the constraint solver
    /// will be the final engraving positions of all Blocks in the System.
    ///
    /// Once all positions have been resolved, Blocks are converted into
    /// Engravables and the result is returned as an EngravedSystem, ready to be
    /// streamed to a Rescore client for display.
    pub fn engrave(&self) -> Result<EngravedSystem, EngravingError> {
        // Determine final layout positions for all lines and blocks on the
        // system layout grid.

        let mut solver = Solver::new();

        // First, create linear constraint variables for all lines and blocks.
        // Grid lines get one variable each (horizontal grid lines have a
        // y position, vertical grid lines an x position), blocks get four variables
        // each (blocks have two sets of x and y positions, representing the
        // (start, top) and (end, bottom) corners of the block).

        let horizontal_grid_line_variables = self
            .get_horizontal_grid_lines()
            .iter()
            .map(|_| Variable::new())
            .collect::<Vec<_>>();

        let vertical_grid_line_variables = self
            .get_vertical_grid_lines()
            .iter()
            .map(|_| Variable::new())
            .collect::<Vec<_>>();

        let block_top_position_variables = self
            .get_blocks()
            .iter()
            .map(|_| Variable::new())
            .collect::<Vec<_>>();

        let block_bottom_position_variables = self
            .get_blocks()
            .iter()
            .map(|_| Variable::new())
            .collect::<Vec<_>>();

        let block_start_position_variables = self
            .get_blocks()
            .iter()
            .map(|_| Variable::new())
            .collect::<Vec<_>>();

        let block_end_position_variables = self
            .get_blocks()
            .iter()
            .map(|_| Variable::new())
            .collect::<Vec<_>>();

        // Express the position for the system origin, (0,0), in terms of
        // constraints on the top-most and leading-most grid lines.
        // All other constraints are ultimately resolved in relation to this
        // origin position, so we need to ensure it is defined.

        if let Some(system_top) = horizontal_grid_line_variables.get(self.get_top_edge()) {
            solver
                .add_constraint(*system_top | EQ(REQUIRED) | 0.0)
                .map_err(|err| {
                    EngravingError::AddConstraintErrorOnHorizontalGridLine(err, self.get_top_edge())
                })?;
        }

        // We constrain the system leading edge to match the aligned start of
        // the system; the aligned start is an edit variable since, depending on
        // the desired system alignment, we may need to adjust its value later
        // to effect an end or center alignment.

        let aligned_start = Variable::new();

        solver
            .add_edit_variable(aligned_start, STRONG)
            .map_err(EngravingError::DefineJustificationError)?;

        solver
            .suggest_value(aligned_start, 0.0)
            .map_err(EngravingError::ApplyJustificationError)?;

        if let Some(system_leading_edge) = vertical_grid_line_variables.get(self.get_leading_edge())
        {
            solver
                .add_constraint(*system_leading_edge | EQ(REQUIRED) | aligned_start)
                .map_err(|err| {
                    EngravingError::AddConstraintErrorOnVerticalGridLine(
                        err,
                        self.get_leading_edge(),
                    )
                })?;
        }

        // Express constraints on lines and blocks in relation to variables,
        // and add those constraints to the solver.

        // When expressing constraints on the vertical axis, we need to be careful
        // about our coordinate system: with the system origin at (0,0),
        // vertical positions closer to the _top_ of the system have a _smaller_
        // y value, with 0 being the top-most position on the system.

        // The linear solver adjusts variables to fit constraints progressively as
        // constraints are added to the system, so by the time all constraints are
        // added, we have our layout solution.

        for (index, grid_line) in self.get_horizontal_grid_lines().iter().enumerate() {
            for constraint in grid_line.get_constraints() {
                Self::add_horizontal_grid_line_constraint_to_solver(
                    index,
                    constraint,
                    &mut solver,
                    horizontal_grid_line_variables.as_slice(),
                )?;
            }
        }

        for (index, grid_line) in self.get_vertical_grid_lines().iter().enumerate() {
            for constraint in grid_line.get_constraints() {
                Self::add_vertical_grid_line_constraint_to_solver(
                    index,
                    constraint,
                    &mut solver,
                    vertical_grid_line_variables.as_slice(),
                )?;
            }
        }

        let mut spacing_blocks = Vec::new();

        let mut total_rhythmic_spacing = STAVE_SPACES_ZERO;

        for (index, block) in self.get_blocks().iter().enumerate() {
            if block.is_spacing_block() {
                // Keep track of the indices of any spacing blocks on the grid.
                // We'll need these later in order to justify the system.

                spacing_blocks.push(index);

                // Keep track of the total amount of rhythmic space currently
                // on the grid. The ratio of rhythmic space to system width
                // is used during system justification.

                total_rhythmic_spacing += block.get_fixed_width();
            }

            // If this block is fixed width, then ensure its width is taken into account
            // when determining its end position.

            if block.is_fixed_width() {
                solver
                    .add_constraint(
                        *block_end_position_variables
                            .get(index)
                            .ok_or(EngravingError::UnknownBlockEndPosition(index))?
                            | EQ(STRONG)
                            | (*block_start_position_variables
                                .get(index)
                                .ok_or(EngravingError::UnknownBlockStartPosition(index))?
                                + block.get_start_padding().value
                                + block.get_fixed_width().value
                                + block.get_end_padding().value),
                    )
                    .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index))?;
            }

            // If this block is fixed height, then ensure its height is taken into account
            // when determining its bottom position.

            if block.is_fixed_height() {
                solver
                    .add_constraint(
                        *block_bottom_position_variables
                            .get(index)
                            .ok_or(EngravingError::UnknownBlockBottomPosition(index))?
                            | EQ(STRONG)
                            | (*block_top_position_variables
                                .get(index)
                                .ok_or(EngravingError::UnknownBlockTopPosition(index))?
                                + block.get_top_padding().value
                                + block.get_fixed_height().value
                                + block.get_bottom_padding().value),
                    )
                    .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index))?;
            }

            // Add all user-specified constraints to the solver.

            for constraint in block.get_constraints() {
                Self::add_block_constraint_to_solver(
                    index,
                    block,
                    constraint,
                    &mut solver,
                    horizontal_grid_line_variables.as_slice(),
                    vertical_grid_line_variables.as_slice(),
                    block_top_position_variables.as_slice(),
                    block_bottom_position_variables.as_slice(),
                    block_start_position_variables.as_slice(),
                    block_end_position_variables.as_slice(),
                )?;
            }
        }

        // Detect and resolve collisions between blocks.

        let collisions = Self::detect_colliding_blocks(
            self.get_blocks(),
            self.horizontal_grid_lines.len(),
            self.vertical_grid_lines.len(),
            &solver,
            block_top_position_variables.as_slice(),
            block_bottom_position_variables.as_slice(),
            block_start_position_variables.as_slice(),
            block_end_position_variables.as_slice(),
        );

        Self::resolve_colliding_blocks(
            self.get_blocks(),
            collisions.as_slice(),
            &mut solver,
            horizontal_grid_line_variables.as_slice(),
            vertical_grid_line_variables.as_slice(),
            block_top_position_variables.as_slice(),
            block_bottom_position_variables.as_slice(),
            block_start_position_variables.as_slice(),
            block_end_position_variables.as_slice(),
        )?;

        // Determine the pre-justification engraved width of the system by scanning
        // the solved block positions for maximal extents.

        let engraved_system_width = block_end_position_variables
            .iter()
            .map(|variable| StaveSpaces::new(solver.get_value(*variable) as f32))
            .max()
            .unwrap_or(STAVE_SPACES_ZERO);

        Self::apply_justification_to_solver(
            self.justification,
            self.target_system_width,
            engraved_system_width,
            total_rhythmic_spacing,
            &mut solver,
            &aligned_start,
            block_start_position_variables.as_slice(),
            block_end_position_variables.as_slice(),
            self.get_blocks(),
            spacing_blocks.as_slice(),
        )?;

        // Retrieve all finalized block positions from solver.

        let block_top_positions = block_top_position_variables
            .iter()
            .map(|variable| StaveSpaces::new(solver.get_value(*variable) as f32))
            .collect::<Vec<_>>();

        let block_bottom_positions = block_bottom_position_variables
            .iter()
            .map(|variable| StaveSpaces::new(solver.get_value(*variable) as f32))
            .collect::<Vec<_>>();

        let block_start_positions = block_start_position_variables
            .iter()
            .map(|variable| StaveSpaces::new(solver.get_value(*variable) as f32))
            .collect::<Vec<_>>();

        let block_end_positions = block_end_position_variables
            .iter()
            .map(|variable| StaveSpaces::new(solver.get_value(*variable) as f32))
            .collect::<Vec<_>>();

        // Determine the final engraved width and height of the system by scanning
        // the solved block positions for maximal extents.

        let width = block_end_positions
            .iter()
            .max()
            .copied()
            .unwrap_or(STAVE_SPACES_ZERO);

        let height = block_bottom_positions
            .iter()
            .max()
            .copied()
            .unwrap_or(STAVE_SPACES_ZERO);

        // With the final position of every Block in the system grid now known,
        // we can create positioned Engravables for each Block and return the
        // completed EngravedSystem.

        let foreground = Self::create_engravables_from_blocks_in_layer(
            self.get_blocks(),
            BlockLayer::Foreground,
            block_top_positions.as_slice(),
            block_bottom_positions.as_slice(),
            block_start_positions.as_slice(),
            block_end_positions.as_slice(),
            self.debug_do_show_rhythmic_spacing,
        );

        let midground = Self::create_engravables_from_blocks_in_layer(
            self.get_blocks(),
            BlockLayer::Midground,
            block_top_positions.as_slice(),
            block_bottom_positions.as_slice(),
            block_start_positions.as_slice(),
            block_end_positions.as_slice(),
            self.debug_do_show_rhythmic_spacing,
        );

        let mut background = Self::create_engravables_from_blocks_in_layer(
            self.get_blocks(),
            BlockLayer::Background,
            block_top_positions.as_slice(),
            block_bottom_positions.as_slice(),
            block_start_positions.as_slice(),
            block_end_positions.as_slice(),
            self.debug_do_show_rhythmic_spacing,
        );

        let horizontal_grid_line_positions = horizontal_grid_line_variables
            .iter()
            .map(|variable| StaveSpaces::new(solver.get_value(*variable) as f32))
            .collect::<Vec<_>>();

        if self.debug_do_draw_horizontal_grid_lines {
            // Add visual guides for horizontal grid lines and output debugging data.

            background.append(
                &mut Self::create_debug_engravables_for_horizontal_grid_lines(
                    self.get_horizontal_grid_lines(),
                    horizontal_grid_line_positions.as_slice(),
                    width,
                ),
            );

            self.get_horizontal_grid_lines()
                .iter()
                .zip(horizontal_grid_line_positions.clone())
                .enumerate()
                .for_each(|(index, (grid_line, position))| {
                    log::debug!(
                        "models::display::layout::system::engrave(): horizontal_grid_line_type = {:?}, index = {}, y = {}",
                        grid_line.get_grid_line_type(),
                        index,
                        position
                    );
                });
        }

        let vertical_grid_line_positions = vertical_grid_line_variables
            .iter()
            .map(|variable| StaveSpaces::new(solver.get_value(*variable) as f32))
            .collect::<Vec<_>>();

        if self.debug_do_draw_vertical_grid_lines {
            // Add visual guides for vertical grid lines and output debugging data.

            background.append(&mut Self::create_debug_engravables_for_vertical_grid_lines(
                self.get_vertical_grid_lines(),
                vertical_grid_line_positions.as_slice(),
                height,
            ));

            self.get_vertical_grid_lines()
                .iter()
                .zip(vertical_grid_line_positions.clone())
                .enumerate()
                .for_each(|(index, (grid_line, position))| {
                    log::debug!(
                        "models::display::layout::system::engrave(): vertical_grid_line_type = {:?}, index = {}, x = {}",
                        grid_line.get_grid_line_type(),
                        index,
                        position
                    );
                });
        }

        if self.debug_do_draw_block_outlines {
            // Add visual guides for block bounding boxes.

            background.append(
                &mut Self::create_debug_engravables_for_block_bounding_boxes(
                    self.get_blocks(),
                    block_top_positions.as_slice(),
                    block_bottom_positions.as_slice(),
                    block_start_positions.as_slice(),
                    block_end_positions.as_slice(),
                ),
            );
        }

        Ok(EngravedSystem::new(
            self.index_in_movement,
            horizontal_grid_line_positions,
            vertical_grid_line_positions,
            self.start_ticks,
            self.end_ticks,
            width,
            height,
            vec![], // TODO: AJRC - 8/9/21 - compute EngravedBar positions
            // AJRC - 5/10/21 - casting metrics contains vec of CastBar, probably useful
            foreground,
            midground,
            background,
        ))
    }

    #[inline]
    fn create_debug_engravables_for_horizontal_grid_lines(
        horizontal_grid_lines: &[HorizontalGridLine],
        positions: &[StaveSpaces],
        width: StaveSpaces,
    ) -> Vec<Engravable> {
        horizontal_grid_lines
            .iter()
            .zip(positions)
            .map(|(grid_line, position)| {
                Engravable::new_line(EngravedLine::new(
                    None,
                    None,
                    None,
                    None,
                    StavePoint::new(STAVE_SPACES_ZERO, *position),
                    StavePoint::new(width, *position),
                    StaveSpaces::new(0.1),
                    match grid_line.get_grid_line_type() {
                        HorizontalGridLineType::SystemTop => Color::RED,
                        HorizontalGridLineType::SystemBottom => Color::RED,
                        _ => Color::BLUE,
                    },
                    StrokeStyle::Dashed,
                ))
            })
            .collect::<Vec<_>>()
    }

    #[inline]
    fn create_debug_engravables_for_vertical_grid_lines(
        vertical_grid_lines: &[VerticalGridLine],
        positions: &[StaveSpaces],
        height: StaveSpaces,
    ) -> Vec<Engravable> {
        vertical_grid_lines
            .iter()
            .zip(positions)
            .map(|(grid_line, position)| {
                Engravable::new_line(EngravedLine::new(
                    None,
                    None,
                    None,
                    None,
                    StavePoint::new(*position, STAVE_SPACES_ZERO),
                    StavePoint::new(*position, height),
                    StaveSpaces::new(0.1),
                    match grid_line.get_grid_line_type() {
                        VerticalGridLineType::SystemStart => Color::RED,
                        VerticalGridLineType::PartGroupNameStart => Color::GREEN_YELLOW,
                        VerticalGridLineType::PartGroupNameEnd => Color::GREEN_YELLOW,
                        VerticalGridLineType::PartNameStart => Color::GREEN_YELLOW,
                        VerticalGridLineType::PartNameEnd => Color::GREEN_YELLOW,
                        VerticalGridLineType::PartStaveBraceStart => Color::CHOCOLATE,
                        VerticalGridLineType::PartStaveBraceEnd => Color::CHOCOLATE,
                        VerticalGridLineType::PartGroupLine => Color::CADET_BLUE,
                        VerticalGridLineType::PartGroupBracketStart => Color::CHOCOLATE,
                        VerticalGridLineType::PartGroupBracketEnd => Color::CHOCOLATE,
                        VerticalGridLineType::SystemicLine => Color::AQUA,
                        VerticalGridLineType::InstrumentLayoutStart => Color::CORNFLOWER_BLUE,
                        VerticalGridLineType::AnteriorStart => Color::RED,
                        VerticalGridLineType::AnteriorEnd => Color::RED,
                        VerticalGridLineType::InteriorStart => Color::RED,
                        VerticalGridLineType::ClefColumnStart => Color::FIREBRICK,
                        VerticalGridLineType::ClefColumnEnd => Color::FIREBRICK,
                        VerticalGridLineType::KeySignatureColumnStart => Color::LAWN_GREEN,
                        VerticalGridLineType::KeySignatureColumnEnd => Color::LAWN_GREEN,
                        VerticalGridLineType::TimeSignatureColumnStart => Color::SANDY_BROWN,
                        VerticalGridLineType::TimeSignatureColumnEnd => Color::SANDY_BROWN,
                        VerticalGridLineType::StemColumnStart => Color::ORANGE,
                        VerticalGridLineType::NoteheadLine0AccidentalStackStart => Color::DEEP_PINK,
                        VerticalGridLineType::NoteheadLine0AccidentalStackEnd => Color::DEEP_PINK,
                        VerticalGridLineType::NoteheadLine0NoteheadStackStart => Color::CYAN,
                        VerticalGridLineType::RhythmicSpacingStart => Color::GREEN,
                        VerticalGridLineType::RhythmicSpacingEnd => Color::GREEN,
                        VerticalGridLineType::LyricSyllableEnd => Color::BLUE,
                        VerticalGridLineType::StemColumnEnd => Color::ORANGE,
                        VerticalGridLineType::BarlineStart => Color::BLUE_VIOLET,
                        VerticalGridLineType::BarlineEnd => Color::BLUE_VIOLET,
                        VerticalGridLineType::InteriorEnd => Color::RED,
                        VerticalGridLineType::PosteriorStart => Color::RED,
                        VerticalGridLineType::PosteriorEnd => Color::RED,
                        VerticalGridLineType::InstrumentLayoutEnd => Color::CORNFLOWER_BLUE,
                        VerticalGridLineType::SystemEnd => Color::RED,
                    },
                    StrokeStyle::Dashed,
                ))
            })
            .collect::<Vec<_>>()
    }

    #[inline]
    fn create_debug_engravables_for_block_bounding_boxes(
        blocks: &[BlockEnum],
        block_top_positions: &[StaveSpaces],
        block_bottom_positions: &[StaveSpaces],
        block_start_positions: &[StaveSpaces],
        block_end_positions: &[StaveSpaces],
    ) -> Vec<Engravable> {
        izip!(
            blocks,
            block_top_positions,
            block_bottom_positions,
            block_start_positions,
            block_end_positions
        )
        .map(|(block, top, bottom, start, end)| {
            Self::create_debug_engravable_for_block_bounding_box(
                block,
                top,
                bottom,
                start,
                end,
                Color::DARK_VIOLET,
            )
        })
        .flatten()
        .collect::<Vec<_>>()
    }

    #[inline]
    fn create_debug_engravable_for_block_bounding_box(
        block: &BlockEnum,
        top: &StaveSpaces,
        bottom: &StaveSpaces,
        start: &StaveSpaces,
        end: &StaveSpaces,
        color: Color,
    ) -> Vec<Engravable> {
        vec![
            Engravable::new_line(EngravedLine::new(
                block.get_source_moment_spine_item().cloned(),
                block.get_source_part_index(),
                block.get_source_voice_index(),
                block.get_source_onset(),
                StavePoint::new(*start, *top),
                StavePoint::new(*end, *top),
                StaveSpaces::new(0.1),
                color,
                StrokeStyle::Solid,
            )),
            Engravable::new_line(EngravedLine::new(
                block.get_source_moment_spine_item().cloned(),
                block.get_source_part_index(),
                block.get_source_voice_index(),
                block.get_source_onset(),
                StavePoint::new(*end, *top),
                StavePoint::new(*end, *bottom),
                StaveSpaces::new(0.1),
                color,
                StrokeStyle::Solid,
            )),
            Engravable::new_line(EngravedLine::new(
                block.get_source_moment_spine_item().cloned(),
                block.get_source_part_index(),
                block.get_source_voice_index(),
                block.get_source_onset(),
                StavePoint::new(*end, *bottom),
                StavePoint::new(*start, *bottom),
                StaveSpaces::new(0.1),
                color,
                StrokeStyle::Solid,
            )),
            Engravable::new_line(EngravedLine::new(
                block.get_source_moment_spine_item().cloned(),
                block.get_source_part_index(),
                block.get_source_voice_index(),
                block.get_source_onset(),
                StavePoint::new(*start, *bottom),
                StavePoint::new(*start, *top),
                StaveSpaces::new(0.1),
                color,
                StrokeStyle::Solid,
            )),
        ]
    }

    #[inline]
    fn add_horizontal_grid_line_constraint_to_solver(
        index: HorizontalGridLineIndex,
        constraint: &HorizontalGridLineConstraint,
        solver: &mut Solver,
        horizontal_grid_line_variables: &[Variable],
    ) -> Result<(), EngravingError> {
        match constraint {
            HorizontalGridLineConstraint::LockAboveHorizontalGridLineByDistance(
                grid_line_below,
                distance,
            ) => solver
                .add_constraint(
                    *horizontal_grid_line_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownHorizontalGridLine(index))?
                        | EQ(STRONG)
                        | (*horizontal_grid_line_variables
                            .get(*grid_line_below)
                            .ok_or(EngravingError::UnknownHorizontalGridLine(index))?
                            - *distance),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnHorizontalGridLine(err, index)),
            HorizontalGridLineConstraint::FloatAboveHorizontalGridLineByDistance(
                grid_line_below,
                distance,
            ) => solver
                .add_constraint(
                    *horizontal_grid_line_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownHorizontalGridLine(index))?
                        | LE(WEAK)
                        | (*horizontal_grid_line_variables
                            .get(*grid_line_below)
                            .ok_or(EngravingError::UnknownHorizontalGridLine(*grid_line_below))?
                            - *distance),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnHorizontalGridLine(err, index)),
            HorizontalGridLineConstraint::LockBelowHorizontalGridLineByDistance(
                grid_line_above,
                distance,
            ) => solver
                .add_constraint(
                    *horizontal_grid_line_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownHorizontalGridLine(index))?
                        | EQ(STRONG)
                        | (*horizontal_grid_line_variables
                            .get(*grid_line_above)
                            .ok_or(EngravingError::UnknownHorizontalGridLine(*grid_line_above))?
                            + *distance),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnHorizontalGridLine(err, index)),
            HorizontalGridLineConstraint::FloatBelowHorizontalGridLineByDistance(
                grid_line_above,
                distance,
            ) => solver
                .add_constraint(
                    *horizontal_grid_line_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownHorizontalGridLine(index))?
                        | GE(WEAK)
                        | (*horizontal_grid_line_variables
                            .get(*grid_line_above)
                            .ok_or(EngravingError::UnknownHorizontalGridLine(*grid_line_above))?
                            + *distance),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnHorizontalGridLine(err, index)),
            HorizontalGridLineConstraint::VerticallyCenterBetweenHorizontalGridLines(
                grid_line_above,
                grid_line_below,
            ) => solver
                .add_constraint(
                    *horizontal_grid_line_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownHorizontalGridLine(index))?
                        | EQ(STRONG)
                        | ((*horizontal_grid_line_variables
                            .get(*grid_line_above)
                            .ok_or(EngravingError::UnknownHorizontalGridLine(*grid_line_above))?
                            + *horizontal_grid_line_variables.get(*grid_line_below).ok_or(
                                EngravingError::UnknownHorizontalGridLine(*grid_line_below),
                            )?)
                            / 2.0),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnHorizontalGridLine(err, index)),
        }
    }

    #[inline]
    fn add_vertical_grid_line_constraint_to_solver(
        index: VerticalGridLineIndex,
        constraint: &VerticalGridLineConstraint,
        solver: &mut Solver,
        vertical_grid_line_variables: &[Variable],
    ) -> Result<(), EngravingError> {
        match constraint {
            VerticalGridLineConstraint::LockBeforeVerticalGridLineByDistance(
                grid_line_after,
                distance,
            ) => solver
                .add_constraint(
                    *vertical_grid_line_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownVerticalGridLine(index))?
                        | EQ(STRONG)
                        | (*vertical_grid_line_variables
                            .get(*grid_line_after)
                            .ok_or(EngravingError::UnknownVerticalGridLine(*grid_line_after))?
                            - *distance),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnVerticalGridLine(err, index)),
            VerticalGridLineConstraint::FloatBeforeVerticalGridLineByDistance(
                grid_line_after,
                distance,
            ) => solver
                .add_constraint(
                    *vertical_grid_line_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownVerticalGridLine(index))?
                        | LE(WEAK)
                        | (*vertical_grid_line_variables
                            .get(*grid_line_after)
                            .ok_or(EngravingError::UnknownVerticalGridLine(*grid_line_after))?
                            - *distance),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnVerticalGridLine(err, index)),
            VerticalGridLineConstraint::LockAfterVerticalGridLineByDistance(
                grid_line_before,
                distance,
            ) => solver
                .add_constraint(
                    *vertical_grid_line_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownVerticalGridLine(index))?
                        | EQ(STRONG)
                        | (*vertical_grid_line_variables
                            .get(*grid_line_before)
                            .ok_or(EngravingError::UnknownVerticalGridLine(*grid_line_before))?
                            + *distance),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnVerticalGridLine(err, index)),
            VerticalGridLineConstraint::FloatAfterVerticalGridLineByDistance(
                grid_line_before,
                distance,
            ) => solver
                .add_constraint(
                    *vertical_grid_line_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownVerticalGridLine(index))?
                        | GE(WEAK)
                        | (*vertical_grid_line_variables
                            .get(*grid_line_before)
                            .ok_or(EngravingError::UnknownVerticalGridLine(*grid_line_before))?
                            + *distance),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnVerticalGridLine(err, index)),
        }
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn add_block_constraint_to_solver(
        index: usize,
        block: &BlockEnum,
        constraint: &BlockConstraint,
        solver: &mut Solver,
        horizontal_grid_line_variables: &[Variable],
        vertical_grid_line_variables: &[Variable],
        block_top_position_variables: &[Variable],
        block_bottom_position_variables: &[Variable],
        block_start_position_variables: &[Variable],
        block_end_position_variables: &[Variable],
    ) -> Result<(), EngravingError> {
        // Apply block constraints. BlockConstraint::Lock* constraints should be represented
        // by a STRONG constraint in the solver; BlockConstraint::Float* constraints should be
        // represented by a WEAK constraint in the solver. This allows lock constraints to
        // override float constraints.

        match constraint {
            BlockConstraint::LockTopToHorizontalGridLine(grid_line_above) => solver
                .add_constraint(
                    *block_top_position_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownBlockTopPosition(index))?
                        | EQ(STRONG)
                        | (*horizontal_grid_line_variables
                            .get(*grid_line_above)
                            .ok_or(EngravingError::UnknownHorizontalGridLine(*grid_line_above))?
                            + block.get_top_padding().value),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::FloatTopAfterHorizontalGridLine(grid_line_above) => solver
                .add_constraint(
                    *block_top_position_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownBlockTopPosition(index))?
                        | GE(WEAK)
                        | (*horizontal_grid_line_variables
                            .get(*grid_line_above)
                            .ok_or(EngravingError::UnknownHorizontalGridLine(*grid_line_above))?
                            + block.get_top_padding().value),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::FloatBottomBeforeHorizontalGridLine(grid_line_below) => {
                // The approach we take to this constraint depends on whether the target
                // block has a fixed or variable height. If it's fixed height, then the
                // block's top has to move; otherwise, the block's height can expand.

                if block.is_fixed_height() {
                    solver
                        .add_constraint(
                            *block_top_position_variables
                                .get(index)
                                .ok_or(EngravingError::UnknownBlockTopPosition(index))?
                                | LE(WEAK)
                                | (*horizontal_grid_line_variables.get(*grid_line_below).ok_or(
                                    EngravingError::UnknownHorizontalGridLine(*grid_line_below),
                                )? - block.get_fixed_height().value
                                    - block.get_bottom_padding().value),
                        )
                        .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index))
                } else {
                    solver
                        .add_constraint(
                            *block_bottom_position_variables
                                .get(index)
                                .ok_or(EngravingError::UnknownBlockBottomPosition(index))?
                                | LE(WEAK)
                                | (*horizontal_grid_line_variables.get(*grid_line_below).ok_or(
                                    EngravingError::UnknownHorizontalGridLine(*grid_line_below),
                                )? - block.get_bottom_padding().value),
                        )
                        .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index))
                }
            }
            BlockConstraint::LockBottomToHorizontalGridLine(grid_line_below) => {
                // The approach we take to this constraint depends on whether the target
                // block has a fixed or variable height. If it's fixed height, then the
                // block's top has to move; otherwise, the block's height can expand.

                if block.is_fixed_height() {
                    solver
                        .add_constraint(
                            *block_top_position_variables
                                .get(index)
                                .ok_or(EngravingError::UnknownBlockTopPosition(index))?
                                | EQ(STRONG)
                                | (*horizontal_grid_line_variables.get(*grid_line_below).ok_or(
                                    EngravingError::UnknownHorizontalGridLine(*grid_line_below),
                                )? - block.get_fixed_height().value
                                    - block.get_bottom_padding().value),
                        )
                        .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index))
                } else {
                    solver
                        .add_constraint(
                            *block_bottom_position_variables
                                .get(index)
                                .ok_or(EngravingError::UnknownBlockBottomPosition(index))?
                                | EQ(STRONG)
                                | (*horizontal_grid_line_variables.get(*grid_line_below).ok_or(
                                    EngravingError::UnknownHorizontalGridLine(*grid_line_below),
                                )? - block.get_bottom_padding().value),
                        )
                        .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index))
                }
            }
            BlockConstraint::LockStartToVerticalGridLine(grid_line_before) => solver
                .add_constraint(
                    *block_start_position_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownBlockStartPosition(index))?
                        | EQ(STRONG)
                        | (*vertical_grid_line_variables
                            .get(*grid_line_before)
                            .ok_or(EngravingError::UnknownVerticalGridLine(*grid_line_before))?
                            + block.get_start_padding().value),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::FloatStartAfterVerticalGridLine(grid_line_before) => solver
                .add_constraint(
                    *block_start_position_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownBlockStartPosition(index))?
                        | GE(WEAK)
                        | (*vertical_grid_line_variables
                            .get(*grid_line_before)
                            .ok_or(EngravingError::UnknownVerticalGridLine(*grid_line_before))?
                            + block.get_start_padding().value),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::FloatEndBeforeVerticalGridLine(grid_line_after) => {
                // The approach we take to this constraint depends on whether the target
                // block has a fixed or variable width. If it's fixed width, then the
                // block's start has to move; otherwise, the block's width can expand.

                if block.is_fixed_width() {
                    solver
                        .add_constraint(
                            *block_start_position_variables
                                .get(index)
                                .ok_or(EngravingError::UnknownBlockStartPosition(index))?
                                | LE(WEAK)
                                | (*vertical_grid_line_variables.get(*grid_line_after).ok_or(
                                    EngravingError::UnknownVerticalGridLine(*grid_line_after),
                                )? - block.get_fixed_width().value
                                    - block.get_end_padding().value),
                        )
                        .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index))
                } else {
                    solver
                        .add_constraint(
                            *block_end_position_variables
                                .get(index)
                                .ok_or(EngravingError::UnknownBlockEndPosition(index))?
                                | LE(WEAK)
                                | (*vertical_grid_line_variables.get(*grid_line_after).ok_or(
                                    EngravingError::UnknownVerticalGridLine(*grid_line_after),
                                )? - block.get_end_padding().value),
                        )
                        .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index))
                }
            }
            BlockConstraint::LockEndToVerticalGridLine(grid_line_after) => {
                // The approach we take to this constraint depends on whether the target
                // block has a fixed or variable width. If it's fixed width, then the
                // block's start has to move; otherwise, the block's width can expand.

                if block.is_fixed_width() {
                    solver
                        .add_constraint(
                            *block_start_position_variables
                                .get(index)
                                .ok_or(EngravingError::UnknownBlockStartPosition(index))?
                                | EQ(STRONG)
                                | (*vertical_grid_line_variables.get(*grid_line_after).ok_or(
                                    EngravingError::UnknownVerticalGridLine(*grid_line_after),
                                )? - block.get_fixed_width().value
                                    - block.get_end_padding().value),
                        )
                        .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index))
                } else {
                    solver
                        .add_constraint(
                            *block_end_position_variables
                                .get(index)
                                .ok_or(EngravingError::UnknownBlockEndPosition(index))?
                                | EQ(STRONG)
                                | (*vertical_grid_line_variables.get(*grid_line_after).ok_or(
                                    EngravingError::UnknownVerticalGridLine(*grid_line_after),
                                )? - block.get_end_padding().value),
                        )
                        .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index))
                }
            }
            BlockConstraint::LockVerticalCenterHalfwayBetweenHorizontalGridLines(
                grid_line_above,
                grid_line_below,
            ) => solver
                .add_constraint(
                    *block_top_position_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownBlockTopPosition(index))?
                        | EQ(STRONG)
                        | ((*horizontal_grid_line_variables
                            .get(*grid_line_above)
                            .ok_or(EngravingError::UnknownHorizontalGridLine(*grid_line_above))?
                            + *horizontal_grid_line_variables.get(*grid_line_below).ok_or(
                                EngravingError::UnknownHorizontalGridLine(*grid_line_below),
                            )?)
                            / 2.0
                            - block.get_descent().value),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::LockVerticalCenterToHorizontalGridLine(grid_line_center) => solver
                .add_constraint(
                    *block_top_position_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownBlockTopPosition(index))?
                        | EQ(STRONG)
                        | (*horizontal_grid_line_variables
                            .get(*grid_line_center)
                            .ok_or(EngravingError::UnknownHorizontalGridLine(*grid_line_center))?
                            - block.get_descent().value),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::LockHorizontalCenterHalfwayBetweenVerticalGridLines(
                grid_line_before,
                grid_line_after,
            ) => {
                solver
                    .add_constraint(
                        *block_start_position_variables
                            .get(index)
                            .ok_or(EngravingError::UnknownBlockStartPosition(index))?
                            | GE(STRONG)
                            | ((*vertical_grid_line_variables.get(*grid_line_before).ok_or(
                                EngravingError::UnknownVerticalGridLine(*grid_line_before),
                            )? + *vertical_grid_line_variables.get(*grid_line_after).ok_or(
                                EngravingError::UnknownVerticalGridLine(*grid_line_after),
                            )? - block.get_fixed_width().value)
                                / 2.0),
                    )
                    .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index))
            }
            BlockConstraint::LockHorizontalCenterToVerticalGridLine(grid_line_center) => solver
                .add_constraint(
                    *block_start_position_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownBlockStartPosition(index))?
                        | EQ(STRONG)
                        | (*vertical_grid_line_variables
                            .get(*grid_line_center)
                            .ok_or(EngravingError::UnknownVerticalGridLine(*grid_line_center))?
                            - block.get_fixed_width().value / 2.0),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::PushHorizontalGridLineDownToAccommodateBlockHeight(
                grid_line_below,
            ) => solver
                .add_constraint(
                    *horizontal_grid_line_variables
                        .get(*grid_line_below)
                        .ok_or(EngravingError::UnknownHorizontalGridLine(*grid_line_below))?
                        | GE(STRONG)
                        | (*block_top_position_variables
                            .get(index)
                            .ok_or(EngravingError::UnknownBlockTopPosition(index))?
                            + block.get_fixed_height().value
                            + block.get_bottom_padding().value),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::PushVerticalGridLineSidewaysToAccommodateBlockWidth(
                grid_line_after,
            ) => solver
                .add_constraint(
                    *vertical_grid_line_variables
                        .get(*grid_line_after)
                        .ok_or(EngravingError::UnknownVerticalGridLine(*grid_line_after))?
                        | GE(STRONG)
                        | (*block_start_position_variables
                            .get(index)
                            .ok_or(EngravingError::UnknownBlockStartPosition(index))?
                            + block.get_fixed_width().value
                            + block.get_end_padding().value),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::FloatAfterBlockByDistance(block_before, distance) => solver
                .add_constraint(
                    *block_start_position_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownBlockStartPosition(index))?
                        | GE(WEAK)
                        | (*block_end_position_variables
                            .get(*block_before)
                            .ok_or(EngravingError::UnknownBlockEndPosition(*block_before))?
                            + *distance),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::FloatBeforeBlockByDistance(block_after, distance) => solver
                .add_constraint(
                    *block_start_position_variables
                        .get(*block_after)
                        .ok_or(EngravingError::UnknownBlockStartPosition(*block_after))?
                        | GE(WEAK)
                        | (*block_end_position_variables
                            .get(index)
                            .ok_or(EngravingError::UnknownBlockEndPosition(index))?
                            + *distance),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::FloatAboveBlockByDistance(block_beneath, distance) => solver
                .add_constraint(
                    *block_top_position_variables
                        .get(*block_beneath)
                        .ok_or(EngravingError::UnknownBlockTopPosition(*block_beneath))?
                        | GE(WEAK)
                        | (*block_bottom_position_variables
                            .get(index)
                            .ok_or(EngravingError::UnknownBlockBottomPosition(index))?
                            + *distance),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::FloatBeneathBlockByDistance(block_above, distance) => solver
                .add_constraint(
                    *block_top_position_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownBlockTopPosition(index))?
                        | GE(WEAK)
                        | (*block_bottom_position_variables
                            .get(*block_above)
                            .ok_or(EngravingError::UnknownBlockBottomPosition(*block_above))?
                            + *distance),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::LockStartToBlockStart(other_block) => solver
                .add_constraint(
                    *block_start_position_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownBlockStartPosition(index))?
                        | EQ(STRONG)
                        | (*block_start_position_variables
                            .get(*other_block)
                            .ok_or(EngravingError::UnknownBlockStartPosition(*other_block))?
                            + block.get_start_padding().value),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::LockEndToBlockEnd(other_block) => solver
                .add_constraint(
                    *block_end_position_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownBlockEndPosition(index))?
                        | EQ(STRONG)
                        | (*block_end_position_variables
                            .get(*other_block)
                            .ok_or(EngravingError::UnknownBlockEndPosition(*other_block))?
                            - block.get_end_padding().value),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::LockTopToBlockTop(other_block) => solver
                .add_constraint(
                    *block_top_position_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownBlockTopPosition(index))?
                        | EQ(STRONG)
                        | (*block_top_position_variables
                            .get(*other_block)
                            .ok_or(EngravingError::UnknownBlockTopPosition(*other_block))?
                            + block.get_top_padding().value),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::LockBottomToBlockBottom(other_block) => solver
                .add_constraint(
                    *block_bottom_position_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownBlockBottomPosition(index))?
                        | EQ(STRONG)
                        | (*block_bottom_position_variables
                            .get(*other_block)
                            .ok_or(EngravingError::UnknownBlockBottomPosition(*other_block))?
                            - block.get_bottom_padding().value),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::LockHorizontalCenterBetweenBlocks(block_before, block_after) => solver
                .add_constraint(
                    *block_start_position_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownBlockStartPosition(index))?
                        | EQ(STRONG)
                        | ((*block_end_position_variables
                            .get(*block_before)
                            .ok_or(EngravingError::UnknownBlockEndPosition(*block_before))?
                            + *block_start_position_variables
                                .get(*block_after)
                                .ok_or(EngravingError::UnknownBlockStartPosition(*block_after))?
                            - block.get_fixed_width().value)
                            / 2.0),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::LockVerticalCenterBetweenBlocks(block_above, block_beneath) => solver
                .add_constraint(
                    *block_top_position_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownBlockTopPosition(index))?
                        | EQ(STRONG)
                        | ((*block_bottom_position_variables
                            .get(*block_above)
                            .ok_or(EngravingError::UnknownBlockBottomPosition(*block_above))?
                            + *block_top_position_variables
                                .get(*block_beneath)
                                .ok_or(EngravingError::UnknownBlockTopPosition(*block_beneath))?
                            - block.get_fixed_height().value)
                            / 2.0),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::LockHorizontalCenterToBlockCenter(other_block) => solver
                .add_constraint(
                    ((*block_start_position_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownBlockStartPosition(index))?
                        + *block_end_position_variables
                            .get(index)
                            .ok_or(EngravingError::UnknownBlockEndPosition(index))?)
                        / 2.0)
                        | EQ(STRONG)
                        | ((*block_start_position_variables
                            .get(*other_block)
                            .ok_or(EngravingError::UnknownBlockStartPosition(*other_block))?
                            + *block_end_position_variables
                                .get(*other_block)
                                .ok_or(EngravingError::UnknownBlockEndPosition(*other_block))?)
                            / 2.0),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::FloatHorizontalCenterToBlockCenter(other_block) => solver
                .add_constraint(
                    ((*block_start_position_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownBlockStartPosition(index))?
                        + *block_end_position_variables
                            .get(index)
                            .ok_or(EngravingError::UnknownBlockEndPosition(index))?)
                        / 2.0)
                        | EQ(WEAK)
                        | ((*block_start_position_variables
                            .get(*other_block)
                            .ok_or(EngravingError::UnknownBlockStartPosition(*other_block))?
                            + *block_end_position_variables
                                .get(*other_block)
                                .ok_or(EngravingError::UnknownBlockEndPosition(*other_block))?)
                            / 2.0),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::LockVerticalCenterToBlockCenter(other_block) => solver
                .add_constraint(
                    ((*block_top_position_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownBlockTopPosition(index))?
                        + *block_bottom_position_variables
                            .get(index)
                            .ok_or(EngravingError::UnknownBlockBottomPosition(index))?)
                        / 2.0)
                        | EQ(STRONG)
                        | ((*block_top_position_variables
                            .get(*other_block)
                            .ok_or(EngravingError::UnknownBlockTopPosition(*other_block))?
                            + *block_bottom_position_variables.get(*other_block).ok_or(
                                EngravingError::UnknownBlockBottomPosition(*other_block),
                            )?)
                            / 2.0),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::LockAfterBlockByDistance(block_before, distance) => solver
                .add_constraint(
                    *block_start_position_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownBlockStartPosition(index))?
                        | EQ(STRONG)
                        | (*block_end_position_variables
                            .get(*block_before)
                            .ok_or(EngravingError::UnknownBlockEndPosition(*block_before))?
                            + *distance),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::LockBeforeBlockByDistance(block_after, distance) => solver
                .add_constraint(
                    *block_end_position_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownBlockEndPosition(index))?
                        | EQ(STRONG)
                        | (*block_start_position_variables
                            .get(*block_after)
                            .ok_or(EngravingError::UnknownBlockStartPosition(*block_after))?
                            - *distance),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::LockAboveBlockByDistance(block_beneath, distance) => solver
                .add_constraint(
                    *block_bottom_position_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownBlockBottomPosition(index))?
                        | EQ(STRONG)
                        | (*block_top_position_variables
                            .get(*block_beneath)
                            .ok_or(EngravingError::UnknownBlockTopPosition(*block_beneath))?
                            - *distance),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::LockBeneathBlockByDistance(block_above, distance) => solver
                .add_constraint(
                    *block_top_position_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownBlockTopPosition(index))?
                        | EQ(STRONG)
                        | (*block_bottom_position_variables
                            .get(*block_above)
                            .ok_or(EngravingError::UnknownBlockBottomPosition(*block_above))?
                            + *distance),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::LockTopToBlockCenter(other_block) => solver
                .add_constraint(
                    *block_top_position_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownBlockTopPosition(index))?
                        | EQ(STRONG)
                        | ((*block_top_position_variables
                            .get(*other_block)
                            .ok_or(EngravingError::UnknownBlockTopPosition(*other_block))?
                            + *block_bottom_position_variables.get(*other_block).ok_or(
                                EngravingError::UnknownBlockBottomPosition(*other_block),
                            )?)
                            / 2.0),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
            BlockConstraint::LockBottomToBlockCenter(other_block) => solver
                .add_constraint(
                    *block_bottom_position_variables
                        .get(index)
                        .ok_or(EngravingError::UnknownBlockBottomPosition(index))?
                        | EQ(STRONG)
                        | ((*block_top_position_variables
                            .get(*other_block)
                            .ok_or(EngravingError::UnknownBlockTopPosition(*other_block))?
                            + *block_bottom_position_variables.get(*other_block).ok_or(
                                EngravingError::UnknownBlockBottomPosition(*other_block),
                            )?)
                            / 2.0),
                )
                .map_err(|err| EngravingError::AddConstraintErrorOnBlock(err, index)),
        }
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn detect_colliding_blocks(
        blocks: &[BlockEnum],
        horizontal_grid_lines_count: usize,
        vertical_grid_lines_count: usize,
        solver: &Solver,
        block_top_position_variables: &[Variable],
        block_bottom_position_variables: &[Variable],
        block_start_position_variables: &[Variable],
        block_end_position_variables: &[Variable],
    ) -> Vec<(BlockIndex, BlockIndex)> {
        // Detect collisions between blocks.

        // Not every block needs to participate in collision detection; we narrow
        // our focus to just those blocks specifically marked as collidable.
        // For each collidable block, we store its computed (start..end) and
        // (top..bottom) positions in index maps representing the x (horizontal) and y (vertical)
        // coordinate planes. We can then scan those index maps to see which blocks overlap
        // in each plane; if two blocks overlap in both planes, we have a collision.

        // First, build index maps containing coordinate ranges in the horizontal and vertical
        // planes for every candidate block.

        let mut x_plane_intervals = IntervalMap::new();

        let mut y_plane_intervals = IntervalMap::new();

        for (index, block) in blocks.iter().enumerate() {
            if block.is_collidable() {
                let start_position = solver.get_value(block_start_position_variables[index]);
                let end_position = solver.get_value(block_end_position_variables[index]);

                if end_position > start_position {
                    x_plane_intervals.insert(start_position..end_position, index);

                    let top_position = solver.get_value(block_top_position_variables[index]);
                    let bottom_position = solver.get_value(block_bottom_position_variables[index]);

                    if bottom_position > top_position {
                        y_plane_intervals.insert(top_position..bottom_position, index);
                    }
                }
            }
        }

        // Next, build a list of colliding blocks by scanning the horizontal and vertical planes
        // for collision candidates. Our first conundrum: which plane should we scan first?

        if horizontal_grid_lines_count > vertical_grid_lines_count {
            // There are more horizontal grid lines than vertical grid lines in this system.
            // This suggests the system is more horizontally dense than vertically dense,
            // and thus collisions are more likely to occur horizontally than vertically.
            // We therefore expect more false vertical collisions than false horizontal collisions.
            // We scan for horizontal collisions first, since this immediately eliminates the
            // greatest number of false positives from consideration.

            Self::detect_colliding_blocks_horizontally(
                blocks,
                x_plane_intervals,
                y_plane_intervals,
                solver,
                block_top_position_variables,
                block_bottom_position_variables,
                block_start_position_variables,
                block_end_position_variables,
            )
        } else {
            // There are more vertical grid lines than horizontal grid lines in this system.
            // This suggests the system is more vertically dense than horizontally dense,
            // and thus collisions are more likely to occur vertically than horizontally.
            // We therefore expect more false horizontal collisions than false vertical collisions.
            // We scan for vertical collisions first, since this immediately eliminates the
            // greatest number of false positives from consideration.

            Self::detect_colliding_blocks_vertically(
                blocks,
                x_plane_intervals,
                y_plane_intervals,
                solver,
                block_top_position_variables,
                block_bottom_position_variables,
                block_start_position_variables,
                block_end_position_variables,
            )
        }
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn detect_colliding_blocks_horizontally(
        blocks: &[BlockEnum],
        x_plane_intervals: IntervalMap<f64, BlockIndex>,
        y_plane_intervals: IntervalMap<f64, BlockIndex>,
        solver: &Solver,
        block_top_position_variables: &[Variable],
        block_bottom_position_variables: &[Variable],
        block_start_position_variables: &[Variable],
        block_end_position_variables: &[Variable],
    ) -> Vec<(BlockIndex, BlockIndex)> {
        // Build a list of colliding blocks by scanning for collisions in the horizontal plane.

        let mut collisions = Vec::new();

        for (index, block) in blocks.iter().enumerate() {
            if block.is_collidable() {
                for (_, horizontal_collision_candidate_index) in x_plane_intervals.iter(
                    solver.get_value(block_start_position_variables[index])
                        ..solver.get_value(block_end_position_variables[index]),
                ) {
                    // We expect blocks to collide with their own coordinates; ignore this.

                    if *horizontal_collision_candidate_index != index {
                        // Additionally, ignore colliding blocks generated from the same source
                        // spine item; we assume any such collisions (e.g. tail flags touching
                        // noteheads) are deliberate. It's only collisions from blocks generated
                        // from _different_ source spine items that concern us.

                        if block.get_source_moment_spine_item()
                            != blocks[*horizontal_collision_candidate_index]
                                .get_source_moment_spine_item()
                        {
                            // This is a valid collision on the horizontal plane. Check to see
                            // if these blocks collide on the vertical plane as well.

                            for (_, vertical_collision_candidate_index) in y_plane_intervals.iter(
                                solver.get_value(block_top_position_variables[index])
                                    ..solver.get_value(block_bottom_position_variables[index]),
                            ) {
                                if vertical_collision_candidate_index
                                    == horizontal_collision_candidate_index
                                {
                                    // These blocks collide on both the horizontal and vertical planes.

                                    collisions.push((index, *vertical_collision_candidate_index));
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        collisions
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn detect_colliding_blocks_vertically(
        blocks: &[BlockEnum],
        x_plane_intervals: IntervalMap<f64, BlockIndex>,
        y_plane_intervals: IntervalMap<f64, BlockIndex>,
        solver: &Solver,
        block_top_position_variables: &[Variable],
        block_bottom_position_variables: &[Variable],
        block_start_position_variables: &[Variable],
        block_end_position_variables: &[Variable],
    ) -> Vec<(BlockIndex, BlockIndex)> {
        // Build a list of colliding blocks by scanning for collisions in the vertical plane.

        let mut collisions = Vec::new();

        for (index, block) in blocks.iter().enumerate() {
            if block.is_collidable() {
                for (_, vertical_collision_candidate_index) in y_plane_intervals.iter(
                    solver.get_value(block_top_position_variables[index])
                        ..solver.get_value(block_bottom_position_variables[index]),
                ) {
                    // We expect blocks to collide with their own coordinates; ignore this.

                    if *vertical_collision_candidate_index != index {
                        // Additionally, ignore colliding blocks generated from the same source
                        // spine item; we assume any such collisions (e.g. tail flags touching
                        // noteheads) are deliberate. It's only collisions from blocks generated
                        // from _different_ source spine items that concern us.

                        if block.get_source_moment_spine_item()
                            != blocks[*vertical_collision_candidate_index]
                                .get_source_moment_spine_item()
                        {
                            // This is a valid collision on the vertical plane. Check to see
                            // if these blocks collide on the horizontal plane as well.

                            for (_, horizontal_collision_candidate_index) in x_plane_intervals.iter(
                                solver.get_value(block_start_position_variables[index])
                                    ..solver.get_value(block_end_position_variables[index]),
                            ) {
                                if horizontal_collision_candidate_index
                                    == vertical_collision_candidate_index
                                {
                                    // These blocks collide on both the horizontal and vertical planes.

                                    collisions.push((index, *horizontal_collision_candidate_index));
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        collisions
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn resolve_colliding_blocks(
        blocks: &[BlockEnum],
        collisions: &[(BlockIndex, BlockIndex)],
        solver: &mut Solver,
        horizontal_grid_line_variables: &[Variable],
        vertical_grid_line_variables: &[Variable],
        block_top_position_variables: &[Variable],
        block_bottom_position_variables: &[Variable],
        block_start_position_variables: &[Variable],
        block_end_position_variables: &[Variable],
    ) -> Result<(), EngravingError> {
        // Resolve block collisions by shifting blocks vertically or horizontally.

        // If either block can move vertically, then it might be able move up or down
        // to avoid collision; the direction of vertical movement is based on the block's
        // source voice index, with blocks sourced from lower-indexed voices moving
        // upwards to avoid blocks sourced from higher-indexed voices. If neither block
        // can move vertically, then push the block with the later start position sideways
        // to avoid collision.

        // Any moved block needs to have collision detection run on it again to make
        // sure we didn't create a new collision while resolving this collision
        // TODO: AJRC - 22/12/21 - we only handle horizontal resolutions for T0
        // TODO: AJRC - 22/12/21 - need to re-run collision detection on adjusted blocks

        for (index_a, index_b) in collisions {
            let index_a = *index_a;

            let index_b = *index_b;

            if blocks[index_a].get_can_move_up_to_avoid_vertical_collision()
                || blocks[index_a].get_can_move_down_to_avoid_vertical_collision()
                || blocks[index_b].get_can_move_up_to_avoid_vertical_collision()
                || blocks[index_b].get_can_move_down_to_avoid_vertical_collision()
            {
                Self::resolve_colliding_blocks_vertically(index_a, index_b)?;
            } else {
                Self::resolve_colliding_blocks_horizontally(
                    index_a,
                    index_b,
                    blocks,
                    solver,
                    horizontal_grid_line_variables,
                    vertical_grid_line_variables,
                    block_top_position_variables,
                    block_bottom_position_variables,
                    block_start_position_variables,
                    block_end_position_variables,
                )?;
            }
        }

        Ok(())
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn resolve_colliding_blocks_vertically(
        index_a: BlockIndex,
        index_b: BlockIndex,
    ) -> Result<(), EngravingError> {
        // TODO: AJRC - 27/12/21 - resolve block collisions vertically.

        log::warn!(
            "models::display::layout::system::resolve_colliding_blocks_vertically(): unresolved vertical collision between block indices {} and {}",
            index_a,
            index_b
        );

        Ok(())
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn resolve_colliding_blocks_horizontally(
        index_a: BlockIndex,
        index_b: BlockIndex,
        blocks: &[BlockEnum],
        solver: &mut Solver,
        horizontal_grid_line_variables: &[Variable],
        vertical_grid_line_variables: &[Variable],
        block_top_position_variables: &[Variable],
        block_bottom_position_variables: &[Variable],
        block_start_position_variables: &[Variable],
        block_end_position_variables: &[Variable],
    ) -> Result<(), EngravingError> {
        if block_start_position_variables[index_a] > block_start_position_variables[index_b] {
            // Add a new constraint to the solver that ensures the first block must start after the second.
            // TODO: AJRC - 22/12/21 - if the blocks are glyphs and are aligned diagonally,
            // then it may be possible to overlap their cut-offs. Check for this.

            Self::add_block_constraint_to_solver(
                index_a,
                &blocks[index_a],
                &BlockConstraint::LockAfterBlockByDistance(index_b, 0.25),
                solver,
                horizontal_grid_line_variables,
                vertical_grid_line_variables,
                block_top_position_variables,
                block_bottom_position_variables,
                block_start_position_variables,
                block_end_position_variables,
            )
        } else {
            // Add a new constraint to the solver that ensures the second block must start after the first.

            Self::add_block_constraint_to_solver(
                index_b,
                &blocks[index_b],
                &BlockConstraint::LockAfterBlockByDistance(index_a, 0.25),
                solver,
                horizontal_grid_line_variables,
                vertical_grid_line_variables,
                block_top_position_variables,
                block_bottom_position_variables,
                block_start_position_variables,
                block_end_position_variables,
            )
        }
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn apply_justification_to_solver(
        justification: SystemJustification,
        target_system_width: StaveSpaces,
        engraved_system_width: StaveSpaces,
        total_rhythmic_spacing: StaveSpaces,
        solver: &mut Solver,
        aligned_start: &Variable,
        block_start_position_variables: &[Variable],
        block_end_position_variables: &[Variable],
        blocks: &[BlockEnum],
        spacing_blocks: &[BlockIndex],
    ) -> Result<(), EngravingError> {
        // Find the maximal vertical grid line position in the solver. That
        // will correspond to the computed system width.

        match justification {
            SystemJustification::AlignStart => {
                // This should already be the default, but there's no harm setting
                // it again; Cassowary won't do anything if the value is the same.

                solver
                    .suggest_value(*aligned_start, 0.0)
                    .map_err(EngravingError::ApplyJustificationError)?;
            }
            SystemJustification::AlignEnd => {
                // Push the aligned start of the system sideways to effect
                // end alignment. The distance we push is
                // (target_system_width - engraved_system_width).

                solver
                    .suggest_value(
                        *aligned_start,
                        (target_system_width.value - engraved_system_width.value) as f64,
                    )
                    .map_err(EngravingError::ApplyJustificationError)?;
            }
            SystemJustification::Centered => {
                // Push the aligned start of the system sideways to effect
                // center alignment. The distance we push is
                // (target_system_width - engraved_system_width) / 2.

                solver
                    .suggest_value(
                        *aligned_start,
                        (target_system_width.value - engraved_system_width.value) as f64 / 2.0,
                    )
                    .map_err(EngravingError::ApplyJustificationError)?;
            }
            SystemJustification::Justified => {
                // Pad the width of each spacing block so that the difference
                // between the target system width and the actual engraved width
                // is evenly spread out over the system.

                if !spacing_blocks.is_empty() {
                    let justification_padding_ratio = (total_rhythmic_spacing.value
                        + target_system_width.value
                        - engraved_system_width.value)
                        / total_rhythmic_spacing.value;

                    for &index in spacing_blocks {
                        if let Some(block) = blocks.get(index) {
                            solver
                                .add_constraint(
                                    *block_end_position_variables
                                        .get(index)
                                        .ok_or(EngravingError::UnknownBlockEndPosition(index))?
                                        | EQ(REQUIRED)
                                        | (*block_start_position_variables.get(index).ok_or(
                                            EngravingError::UnknownBlockStartPosition(index),
                                        )? + block.get_fixed_width().value
                                            * justification_padding_ratio),
                                )
                                .map_err(|err| {
                                    EngravingError::AddConstraintErrorOnBlock(err, index)
                                })?;
                        }
                    }
                }
            }
        };

        Ok(())
    }

    #[inline]
    fn create_engravables_from_blocks_in_layer(
        blocks: &[BlockEnum],
        layer: BlockLayer,
        block_top_positions: &[StaveSpaces],
        block_bottom_positions: &[StaveSpaces],
        block_start_positions: &[StaveSpaces],
        block_end_positions: &[StaveSpaces],
        debug_do_show_rhythmic_spacing: bool,
    ) -> Vec<Engravable> {
        izip!(
            blocks,
            block_top_positions,
            block_bottom_positions,
            block_start_positions,
            block_end_positions
        )
        .filter(|(block, _, _, _, _)| {
            block.get_layer() == layer
                && block.is_visible()
                && (debug_do_show_rhythmic_spacing || !block.is_spacing_block())
        })
        .map(|(block, top, bottom, start, end)| {
            Engravable::new_from_block(block, *top, *bottom, *start, *end)
        })
        .collect::<Vec<_>>()
    }
}

#[derive(Debug, Copy, Clone)]
pub enum EngravingError {
    UnknownHorizontalGridLine(HorizontalGridLineIndex),
    UnknownVerticalGridLine(VerticalGridLineIndex),
    UnknownBlockTopPosition(BlockIndex),
    UnknownBlockBottomPosition(BlockIndex),
    UnknownBlockStartPosition(BlockIndex),
    UnknownBlockEndPosition(BlockIndex),
    AddConstraintErrorOnHorizontalGridLine(AddConstraintError, HorizontalGridLineIndex),
    AddConstraintErrorOnVerticalGridLine(AddConstraintError, VerticalGridLineIndex),
    AddConstraintErrorOnBlock(AddConstraintError, BlockIndex),
    DefineJustificationError(AddEditVariableError),
    ApplyJustificationError(SuggestValueError),
}

impl Display for EngravingError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                EngravingError::UnknownHorizontalGridLine(index) =>
                    format!("Unknown horizontal grid line variable index: {}", index),
                EngravingError::UnknownVerticalGridLine(index) =>
                    format!("Unknown vertical grid line variable index: {}", index),
                EngravingError::UnknownBlockTopPosition(index) =>
                    format!("Unknown block top position variable index: {}", index),
                EngravingError::UnknownBlockBottomPosition(index) =>
                    format!("Unknown block bottom position variable index: {}", index),
                EngravingError::UnknownBlockStartPosition(index) =>
                    format!("Unknown block start position variable index: {}", index),
                EngravingError::UnknownBlockEndPosition(index) =>
                    format!("Unknown block end position variable index: {}", index),
                EngravingError::AddConstraintErrorOnHorizontalGridLine(err, index) => match err {
                    AddConstraintError::DuplicateConstraint => format!(
                        "Error processing constraint on horizontal grid line {}: Duplicate constraint",
                        index
                    ),
                    AddConstraintError::UnsatisfiableConstraint =>
                        format!("Error processing constraint on horizontal grid line {}: Unsatisfiable constraint", index),
                    AddConstraintError::InternalSolverError(err) =>
                        format!("Error processing constraint on horizontal grid line {}: Internal solver error: {}",index, err),
                },
                EngravingError::AddConstraintErrorOnVerticalGridLine(err, index) => match err {
                    AddConstraintError::DuplicateConstraint =>
                        format!("Error processing constraint on vertical grid line {}: Duplicate constraint", index),
                    AddConstraintError::UnsatisfiableConstraint =>
                        format!("Error processing constraint on vertical grid line {}: Unsatisfiable constraint", index),
                    AddConstraintError::InternalSolverError(err) =>
                        format!("Error processing constraint on vertical grid line {}: Internal solver error: {}", index, err),
                },
                EngravingError::AddConstraintErrorOnBlock(err, index) => match err {
                    AddConstraintError::DuplicateConstraint =>
                        format!("Error processing constraint on block {}: Duplicate constraint", index),
                    AddConstraintError::UnsatisfiableConstraint =>
                        format!("Error processing constraint on block {}: Unsatisfiable constraint", index),
                    AddConstraintError::InternalSolverError(err) =>
                        format!("Error processing constraint on block {}: Internal solver error: {}", index, err),
                },
                EngravingError::DefineJustificationError(err) => match err {
                    AddEditVariableError::DuplicateEditVariable =>
                        "Error defining system justification: Duplicate edit variable".to_string(),
                    AddEditVariableError::BadRequiredStrength =>
                        "Error defining system justification: Invalid required strength".to_string(),
                },
                EngravingError::ApplyJustificationError(err) => match err {
                    SuggestValueError::UnknownEditVariable =>
                        "Error applying system justification: Unknown edit variable".to_string(),
                    SuggestValueError::InternalSolverError(err) =>
                        format!("Error applying system justification: Internal solver error: {}", err),
                }
            }
        )
    }
}

impl Error for EngravingError {}

#[cfg(test)]
pub mod tests {
    use crate::models::display::concepts::border::Border;
    use crate::models::display::concepts::color::Color;
    use crate::models::display::concepts::markup::MarkedUpLine;
    use crate::models::display::concepts::stave_spaces::{
        AsStaveSpacesExt, StaveSpaces, STAVE_SPACES_ZERO,
    };
    use crate::models::display::concepts::stroke::StrokeStyle;
    use crate::models::display::engraving::engravable::EngravableItem;
    use crate::models::display::engraving::region::system::EngravedSystem;
    use crate::models::display::glyphs::bravura::Bravura;
    use crate::models::display::glyphs::smufl_font::SmuflFont;
    use crate::models::display::glyphs::Glyph;
    use crate::models::display::grid::horizontal::{
        HorizontalGridLine, HorizontalGridLineIndex, HorizontalGridLineType,
    };
    use crate::models::display::grid::vertical::{
        VerticalGridLine, VerticalGridLineIndex, VerticalGridLineType,
    };
    use crate::models::display::layout::block::glyph::GlyphBlock;
    use crate::models::display::layout::block::line::LineBlock;
    use crate::models::display::layout::block::markup::MarkupBlock;
    use crate::models::display::layout::block::spacing::SpacingBlock;
    use crate::models::display::layout::block::{Block, BlockLayer};
    use crate::models::display::layout::system::{BlockIndex, EngravingError, LayoutSystem};
    use crate::models::display::stylesheet::stylesheet_option::SystemJustification;
    use crate::models::music::concepts::ticks::{AsTicksExt, Ticks, TICKS_ZERO};
    use crate::protos::display::concepts::LineLayout;
    use crate::protos::music::concepts::NotatedDuration;

    #[test]
    fn test_engrave() {
        // Simulate, by constructing blocks and grid lines by hand, a system containing
        // two bars of 2/4 in two voices across two staves. Check computed engraved positions.

        let font = Bravura::new();

        let column_separation = 0.25.as_stave_spaces();

        let stave_separation = 3.as_stave_spaces();

        let rhythmic_space_separation = 1.5.as_stave_spaces();

        let h0_system_top = HorizontalGridLine::new(HorizontalGridLineType::SystemTop);

        let mut h1_system_bottom = HorizontalGridLine::new(HorizontalGridLineType::SystemBottom);

        let v0_system_start = VerticalGridLine::new(0, VerticalGridLineType::SystemStart);

        let mut v1_systemic_line = VerticalGridLine::new(0, VerticalGridLineType::SystemicLine);

        v1_systemic_line.lock_to_grid_line(0);

        let mut v2_system_end = VerticalGridLine::new(1, VerticalGridLineType::SystemEnd);

        // Create grid lines and blocks for stavelines on stave 1.

        let mut h2_s1_l5 = HorizontalGridLine::new(HorizontalGridLineType::Staveline5);
        let mut h3_s1_l4 = HorizontalGridLine::new(HorizontalGridLineType::Staveline4);
        let mut h4_s1_l3 = HorizontalGridLine::new(HorizontalGridLineType::Staveline3);
        let mut h5_s1_l2 = HorizontalGridLine::new(HorizontalGridLineType::Staveline2);
        let mut h6_s1_l1 = HorizontalGridLine::new(HorizontalGridLineType::Staveline1);

        h2_s1_l5.lock_to_grid_line(0);
        h3_s1_l4.lock_below_grid_line(2, 1.as_stave_spaces());
        h4_s1_l3.lock_below_grid_line(3, 1.as_stave_spaces());
        h5_s1_l2.lock_below_grid_line(4, 1.as_stave_spaces());
        h6_s1_l1.lock_below_grid_line(5, 1.as_stave_spaces());

        let b0_s1_l5 = create_staveline_block(2, 1, 2);
        let b1_s1_l4 = create_staveline_block(3, 1, 2);
        let b2_s1_l3 = create_staveline_block(4, 1, 2);
        let b3_s1_l2 = create_staveline_block(5, 1, 2);
        let b4_s1_l1 = create_staveline_block(6, 1, 2);

        // Create lyric underlay grid lines between staves 1 and 2.

        let mut h12_lyric_top =
            HorizontalGridLine::new(HorizontalGridLineType::LyricBelowStaveLine1Top);

        h12_lyric_top.lock_below_grid_line(6, stave_separation);

        let mut h13_lyric_bottom =
            HorizontalGridLine::new(HorizontalGridLineType::LyricBelowStaveLine1Bottom);

        h13_lyric_bottom.float_below_grid_line(12, 1.as_stave_spaces());

        // Create grid lines and blocks for stavelines on stave 2.

        let mut h7_s2_l5 = HorizontalGridLine::new(HorizontalGridLineType::Staveline5);
        let mut h8_s2_l4 = HorizontalGridLine::new(HorizontalGridLineType::Staveline4);
        let mut h9_s2_l3 = HorizontalGridLine::new(HorizontalGridLineType::Staveline3);
        let mut h10_s2_l2 = HorizontalGridLine::new(HorizontalGridLineType::Staveline2);
        let mut h11_s2_l1 = HorizontalGridLine::new(HorizontalGridLineType::Staveline1);

        h7_s2_l5.lock_below_grid_line(13, stave_separation);
        h8_s2_l4.lock_below_grid_line(7, 1.as_stave_spaces());
        h9_s2_l3.lock_below_grid_line(8, 1.as_stave_spaces());
        h10_s2_l2.lock_below_grid_line(9, 1.as_stave_spaces());
        h11_s2_l1.lock_below_grid_line(10, 1.as_stave_spaces());

        let b5_s2_l5 = create_staveline_block(7, 1, 2);
        let b6_s2_l4 = create_staveline_block(8, 1, 2);
        let b7_s2_l3 = create_staveline_block(9, 1, 2);
        let b8_s2_l2 = create_staveline_block(10, 1, 2);
        let b9_s2_l1 = create_staveline_block(11, 1, 2);

        h1_system_bottom.lock_to_grid_line(11);

        let b10_systemic_line = create_systemic_line_block(0, 1, 1);

        // Place blocks on staves in relation to stavelines and columns.

        // First bar of 2/4. Let's put a clef and time signature on each stave.

        let mut v3_bar1_clef_start =
            VerticalGridLine::new(2, VerticalGridLineType::ClefColumnStart);

        v3_bar1_clef_start.float_after_grid_line(1, column_separation);

        let mut v4_bar1_clef_end = VerticalGridLine::new(2, VerticalGridLineType::ClefColumnEnd);

        v4_bar1_clef_end.float_after_grid_line(3, STAVE_SPACES_ZERO);

        let b11_bar1_stave1_clef =
            create_glyph_block_on_staveline(5, 3, 4, TICKS_ZERO, &font, Glyph::GClef);

        let b12_bar1_stave2_clef =
            create_glyph_block_on_staveline(8, 3, 4, TICKS_ZERO, &font, Glyph::FClef);

        let mut v5_bar1_time_sig_start =
            VerticalGridLine::new(3, VerticalGridLineType::TimeSignatureColumnStart);

        v5_bar1_time_sig_start.float_after_grid_line(4, column_separation);

        let mut v6_bar1_time_sig_end =
            VerticalGridLine::new(3, VerticalGridLineType::TimeSignatureColumnEnd);

        v6_bar1_time_sig_end.float_after_grid_line(5, STAVE_SPACES_ZERO);

        let b13_bar1_stave1_time_sig_numerator =
            create_glyph_block_on_staveline(3, 5, 6, TICKS_ZERO, &font, Glyph::TimeSig2Numerator);

        let b14_bar1_stave1_time_sig_denominator =
            create_glyph_block_on_staveline(5, 5, 6, TICKS_ZERO, &font, Glyph::TimeSig4Denominator);

        let b15_bar1_stave2_time_sig_numerator =
            create_glyph_block_on_staveline(8, 5, 6, TICKS_ZERO, &font, Glyph::TimeSig2Numerator);

        let b16_bar1_stave2_time_sig_denominator = create_glyph_block_on_staveline(
            10,
            5,
            6,
            TICKS_ZERO,
            &font,
            Glyph::TimeSig4Denominator,
        );

        // In this test, we can only create noteheads on stavelines (not above or below
        // stavelines), and we do not include stems, so our test musical data is
        // rather artificial. The musical content will be:

        // voice 1 = { G2 T:2/4 g4 bes | g ees | }
        // voice 2 = { G2 T:2/4 f2 | d }

        // Add noteheads in bar 1, voice 1.

        let mut v7_bar1_note1_start =
            VerticalGridLine::new(4, VerticalGridLineType::NoteheadLine0NoteheadStackStart);

        v7_bar1_note1_start.float_after_grid_line(6, column_separation);

        let mut v8_bar1_note1_end =
            VerticalGridLine::new(4, VerticalGridLineType::NoteheadLine0NoteheadStackStart);

        v8_bar1_note1_end.float_after_grid_line(7, STAVE_SPACES_ZERO);

        let mut b17_bar1_voice1_notehead1 =
            create_glyph_block_on_staveline(5, 7, 8, TICKS_ZERO, &font, Glyph::NoteheadBlack);

        b17_bar1_voice1_notehead1.set_end_padding(rhythmic_space_separation); // Simulate rhythmic padding.

        let mut v9_bar1_note2_start =
            VerticalGridLine::new(5, VerticalGridLineType::NoteheadLine0NoteheadStackStart);

        v9_bar1_note2_start.float_after_grid_line(8, column_separation);

        let mut v10_bar1_note2_end =
            VerticalGridLine::new(5, VerticalGridLineType::NoteheadLine0NoteheadStackStart);

        v10_bar1_note2_end.float_after_grid_line(9, STAVE_SPACES_ZERO);

        let mut b18_bar1_voice1_notehead2 = create_glyph_block_on_staveline(
            4,
            9,
            10,
            NotatedDuration::Crotchet.as_ticks(),
            &font,
            Glyph::NoteheadBlack,
        );

        b18_bar1_voice1_notehead2.set_end_padding(rhythmic_space_separation); // Simulate rhythmic padding.

        // Add notehead in bar 1, voice 2.

        let b19_bar1_voice2_notehead =
            create_glyph_block_on_staveline(8, 7, 8, TICKS_ZERO, &font, Glyph::NoteheadHalf);

        // Add barline at end of bar 1.

        let mut v11_bar1_barline_start =
            VerticalGridLine::new(6, VerticalGridLineType::BarlineStart);

        v11_bar1_barline_start.float_after_grid_line(10, column_separation);

        let mut v12_bar1_barline_end = VerticalGridLine::new(6, VerticalGridLineType::BarlineEnd);

        v12_bar1_barline_end.float_after_grid_line(11, STAVE_SPACES_ZERO);

        let b20_bar1_barline =
            create_barline_block(0, 1, 11, 12, NotatedDuration::Minim.as_ticks());

        // Add noteheads in bar 2, voice 1.

        let mut v13_bar2_note1_start =
            VerticalGridLine::new(7, VerticalGridLineType::NoteheadLine0NoteheadStackStart);

        v13_bar2_note1_start.float_after_grid_line(12, column_separation);

        let mut v14_bar2_note1_end =
            VerticalGridLine::new(7, VerticalGridLineType::NoteheadLine0NoteheadStackStart);

        v14_bar2_note1_end.float_after_grid_line(13, STAVE_SPACES_ZERO);

        let mut b21_bar2_voice1_notehead1 = create_glyph_block_on_staveline(
            5,
            13,
            14,
            NotatedDuration::Minim.as_ticks(),
            &font,
            Glyph::NoteheadBlack,
        );

        b21_bar2_voice1_notehead1.set_end_padding(rhythmic_space_separation); // Simulate rhythmic padding.

        let mut v15_bar2_note2_start =
            VerticalGridLine::new(8, VerticalGridLineType::NoteheadLine0NoteheadStackStart);

        v15_bar2_note2_start.float_after_grid_line(14, column_separation);

        let mut v16_bar2_note2_end =
            VerticalGridLine::new(8, VerticalGridLineType::NoteheadLine0NoteheadStackStart);

        v16_bar2_note2_end.float_after_grid_line(15, STAVE_SPACES_ZERO);

        let mut b22_bar2_voice1_notehead2 = create_glyph_block_on_staveline(
            6,
            15,
            16,
            NotatedDuration::Minim * 1.5,
            &font,
            Glyph::NoteheadBlack,
        );

        b22_bar2_voice1_notehead2.set_end_padding(rhythmic_space_separation); // Simulate rhythmic padding.

        // Add notehead in bar 2, voice 2.

        let b23_bar1_voice2_notehead2 = create_glyph_block_on_staveline(
            9,
            13,
            14,
            NotatedDuration::Minim.as_ticks(),
            &font,
            Glyph::NoteheadHalf,
        );

        // Add barline at end of bar 2.

        let mut v17_bar2_barline_start =
            VerticalGridLine::new(9, VerticalGridLineType::BarlineStart);

        v17_bar2_barline_start.float_after_grid_line(16, column_separation);

        let mut v18_bar2_barline_end = VerticalGridLine::new(9, VerticalGridLineType::BarlineEnd);

        v18_bar2_barline_end.float_after_grid_line(17, STAVE_SPACES_ZERO);

        let b24_bar2_barline =
            create_barline_block(0, 1, 17, 18, NotatedDuration::Minim.as_ticks());

        // Create lyrics underneath voice 1 noteheads in bar 1. To do this,
        // we create a vertical grid line locked at the center of the target notehead,
        // then center a markup block containing the lyric on that grid line.
        // We float the lyric inside the grid lines that denote the start and end
        // of each notehead's containing column.

        let v19_bar1_voice1_notehead1_center =
            VerticalGridLine::new(4, VerticalGridLineType::NoteheadLine0NoteheadStackStart);

        b17_bar1_voice1_notehead1.lock_horizontal_center_to_grid_line(19);

        let b25_bar1_lyric1 = create_lyric_underlay_block(12, 13, 7, 19, 8, "A");

        let v20_bar1_voice1_notehead2_center =
            VerticalGridLine::new(5, VerticalGridLineType::NoteheadLine0NoteheadStackStart);

        b18_bar1_voice1_notehead2.lock_horizontal_center_to_grid_line(20);

        let b26_bar1_lyric2 = create_lyric_underlay_block(12, 13, 9, 20, 10, "ve");

        // Similarly, create lyrics underneath voice 1 noteheads in bar2.
        // Let's make the lyric underneath the first notehead a silly length, to test
        // that the grid lines either side of the notehead push apart to accommodate it.

        let v21_bar2_voice1_notehead1_center =
            VerticalGridLine::new(7, VerticalGridLineType::NoteheadLine0NoteheadStackStart);

        b21_bar2_voice1_notehead1.lock_horizontal_center_to_grid_line(21);

        let b27_bar2_lyric1 =
            create_lyric_underlay_block(12, 13, 13, 21, 14, "A lyric of very silly length");

        let v22_bar2_voice1_notehead2_center =
            VerticalGridLine::new(8, VerticalGridLineType::NoteheadLine0NoteheadStackStart);

        b22_bar2_voice1_notehead2.lock_horizontal_center_to_grid_line(22);

        let b28_bar2_lyric2 = create_lyric_underlay_block(12, 13, 15, 22, 16, "Short");

        // Connect the end of the system to the trailing edge of the second barline.
        // Since all stavelines are connected to the end of the system, this will
        // set the width of all stavelines.

        v2_system_end.float_after_grid_line(18, STAVE_SPACES_ZERO);

        // Add all grid lines and blocks to layout.

        let layout = LayoutSystem::new(
            0,
            0.as_ticks(),
            0.as_ticks(),
            SystemJustification::AlignStart,
            100.as_stave_spaces(),
            vec![
                h0_system_top,
                h1_system_bottom,
                h2_s1_l5,
                h3_s1_l4,
                h4_s1_l3,
                h5_s1_l2,
                h6_s1_l1,
                h7_s2_l5,
                h8_s2_l4,
                h9_s2_l3,
                h10_s2_l2,
                h11_s2_l1,
                h12_lyric_top,
                h13_lyric_bottom,
            ],
            vec![
                v0_system_start,
                v1_systemic_line,
                v2_system_end,
                v3_bar1_clef_start,
                v4_bar1_clef_end,
                v5_bar1_time_sig_start,
                v6_bar1_time_sig_end,
                v7_bar1_note1_start,
                v8_bar1_note1_end,
                v9_bar1_note2_start,
                v10_bar1_note2_end,
                v11_bar1_barline_start,
                v12_bar1_barline_end,
                v13_bar2_note1_start,
                v14_bar2_note1_end,
                v15_bar2_note2_start,
                v16_bar2_note2_end,
                v17_bar2_barline_start,
                v18_bar2_barline_end,
                v19_bar1_voice1_notehead1_center,
                v20_bar1_voice1_notehead2_center,
                v21_bar2_voice1_notehead1_center,
                v22_bar2_voice1_notehead2_center,
            ],
            0,
            0,
            vec![
                b0_s1_l5.into(),
                b1_s1_l4.into(),
                b2_s1_l3.into(),
                b3_s1_l2.into(),
                b4_s1_l1.into(),
                b5_s2_l5.into(),
                b6_s2_l4.into(),
                b7_s2_l3.into(),
                b8_s2_l2.into(),
                b9_s2_l1.into(),
                b10_systemic_line.into(),
                b11_bar1_stave1_clef.into(),
                b12_bar1_stave2_clef.into(),
                b13_bar1_stave1_time_sig_numerator.into(),
                b14_bar1_stave1_time_sig_denominator.into(),
                b15_bar1_stave2_time_sig_numerator.into(),
                b16_bar1_stave2_time_sig_denominator.into(),
                b17_bar1_voice1_notehead1.into(),
                b18_bar1_voice1_notehead2.into(),
                b19_bar1_voice2_notehead.into(),
                b20_bar1_barline.into(),
                b21_bar2_voice1_notehead1.into(),
                b22_bar2_voice1_notehead2.into(),
                b23_bar1_voice2_notehead2.into(),
                b24_bar2_barline.into(),
                b25_bar1_lyric1.into(),
                b26_bar1_lyric2.into(),
                b27_bar2_lyric1.into(),
                b28_bar2_lyric2.into(),
            ],
            false,
            false,
            false,
            false,
        );

        // Check computed engraved positions.

        let solution = layout.engrave();

        assert!(solution.is_ok());

        // Let's check the staveline positions first.

        assert_eq!(unwrap_h_line(&solution, 0), STAVE_SPACES_ZERO); // h0_system_top
        assert_eq!(unwrap_h_line(&solution, 1), unwrap_h_line(&solution, 11)); // h1_system_bottom should be aligned to last staveline, h11_s2_l1
        assert_eq!(unwrap_h_line(&solution, 2), unwrap_h_line(&solution, 0)); // h2_s1_l5 should be at the system top
        assert_eq!(
            unwrap_h_line(&solution, 3),
            unwrap_h_line(&solution, 2) + 1.as_stave_spaces()
        ); // h3_s1_l4 == h2_s1_l5 + 1
        assert_eq!(
            unwrap_h_line(&solution, 4),
            unwrap_h_line(&solution, 3) + 1.as_stave_spaces()
        ); // h4_s1_l3 == h3_s1_l4 + 1
        assert_eq!(
            unwrap_h_line(&solution, 5),
            unwrap_h_line(&solution, 4) + 1.as_stave_spaces()
        ); // h5_s1_l2 == h4_s1_l3 + 1
        assert_eq!(
            unwrap_h_line(&solution, 6),
            unwrap_h_line(&solution, 5) + 1.as_stave_spaces()
        ); // h6_s1_l1 == h5_s1_l2 + 1
        assert_eq!(
            unwrap_h_line(&solution, 12),
            unwrap_h_line(&solution, 6) + stave_separation
        ); // h12_lyric_top == h6_s1_l1 + stave_separation
        assert_eq!(
            unwrap_h_line(&solution, 13),
            unwrap_h_line(&solution, 12) + 1.as_stave_spaces()
        ); // h13_lyric_bottom == h12_lyric_top + 1
        assert_eq!(
            unwrap_h_line(&solution, 7),
            unwrap_h_line(&solution, 13) + stave_separation
        ); // h7_s2_l5 == h13_lyric_bottom + stave_separation
        assert_eq!(
            unwrap_h_line(&solution, 8),
            unwrap_h_line(&solution, 7) + 1.as_stave_spaces()
        ); // h8_s2_l4 == h7_s2_l5 + 1
        assert_eq!(
            unwrap_h_line(&solution, 9),
            unwrap_h_line(&solution, 8) + 1.as_stave_spaces()
        ); // h9_s2_13 == h8_s2_l4 + 1
        assert_eq!(
            unwrap_h_line(&solution, 10),
            unwrap_h_line(&solution, 9) + 1.as_stave_spaces()
        ); // h10_s2_l2 == h9_s2_l3 + 1
        assert_eq!(
            unwrap_h_line(&solution, 11),
            unwrap_h_line(&solution, 10) + 1.as_stave_spaces()
        ); // h11_s2_l1 == h10_s2_l2 + 1

        // Ok, the computed grid line positions look good; now let's check that the
        // blocks for the lines are actually positioned on those grid lines.

        let staveline_blocks = vec![
            // Tuples are (block index, HorizontalGridLineIndex of stave line)
            (0, 2),
            (1, 3),
            (2, 4),
            (3, 5),
            (4, 6),
            (5, 7),
            (6, 8),
            (7, 9),
            (8, 10),
            (9, 11),
        ];

        for (block_index, grid_line_index) in staveline_blocks {
            assert_eq!(
                unwrap_block_top(&solution, block_index),
                unwrap_h_line(&solution, grid_line_index)
            );
            assert_eq!(
                unwrap_block_bottom(&solution, block_index),
                unwrap_h_line(&solution, grid_line_index)
            );

            // While we're at it, also check that the start and end positions
            // of the block match the systemic line and system end grid lines.

            assert_eq!(
                unwrap_block_start(&solution, block_index),
                unwrap_v_line(&solution, 1)
            );
            assert_eq!(
                unwrap_block_end(&solution, block_index),
                unwrap_v_line(&solution, 2)
            );
        }

        // Check that the systemic line has expanded to cover the entire vertical
        // range of the system.

        assert_eq!(unwrap_block_start(&solution, 10), STAVE_SPACES_ZERO);
        assert_eq!(unwrap_block_end(&solution, 10), STAVE_SPACES_ZERO);
        assert_eq!(unwrap_block_top(&solution, 10), unwrap_h_line(&solution, 0));
        assert_eq!(
            unwrap_block_bottom(&solution, 10),
            unwrap_h_line(&solution, 1)
        );

        // Now, let's check the vertical grid line positions. We want to be sure
        // that no columns overlap / collide. So long as every vertical grid line
        // in sequence has a horizontal position greater than, or equal to, the
        // preceding vertical grid line, then we can be certain that no columns overlap.

        let ordered_vertical_grid_lines = vec![
            // The order in which we expect the vertical grid lines to appear,
            // from system start to system end.
            0,  // v0_system_start,
            1,  // v1_systemic_line,
            3,  // v3_bar1_clef_start,
            4,  // v4_bar1_clef_end,
            5,  // v5_bar1_time_sig_start,
            6,  // v6_bar1_time_sig_end,
            7,  // v7_bar1_note1_start,
            19, // v19_bar1_voice1_notehead1_center,
            8,  // v8_bar1_note1_end,
            9,  // v9_bar1_note2_start,
            20, // v20_bar1_voice1_notehead2_center,
            10, // v10_bar1_note2_end,
            11, // v11_bar1_barline_start,
            12, // v12_bar1_barline_end,
            13, // v13_bar2_note1_start,
            21, // v21_bar2_voice1_notehead1_center,
            14, // v14_bar2_note1_end,
            15, // v15_bar2_note2_start,
            22, // v22_bar2_voice1_notehead2_center,
            16, // v16_bar2_note2_end,
            17, // v17_bar2_barline_start,
            18, // v18_bar2_barline_end,
            2,  // v2_system_end,
        ];

        for (index, grid_line) in ordered_vertical_grid_lines.iter().enumerate() {
            // Confirm that the position of this grid line ...

            let this_grid_line_position = unwrap_v_line(&solution, *grid_line as usize);

            // ... is greater than or equal to the position of the previous grid line
            // in the sequence.

            if index > 0 {
                if let Some(previous_grid_line_position) = ordered_vertical_grid_lines
                    .get(index - 1)
                    .map(|grid_line| unwrap_v_line(&solution, *grid_line as usize))
                {
                    assert!(this_grid_line_position >= previous_grid_line_position);
                }
            }
        }

        // Next, check the positioning of blocks. We already know that no columns
        // overlap / collide. So, if every block is correctly positioned within its
        // designated column, then it follows that no blocks are colliding either.

        let blocks_in_columns = vec![
            // The vertical grid lines between which each block should be placed.
            // Tuple is (block index, index of grid line before block, index of grid line after block)
            (11, 3, 4), // b11_bar1_stave1_clef between v3_bar1_clef_start and v4_bar1_clef_end
            (12, 3, 4), // b12_bar1_stave2_clef between v3_bar1_clef_start and v4_bar1_clef_end
            (13, 5, 6), // b13_bar1_stave1_time_sig_numerator between v5_bar1_time_sig_start and v6_bar1_time_sig_end
            (14, 5, 6), // b14_bar1_stave1_time_sig_denominator between v5_bar1_time_sig_start and v6_bar1_time_sig_end
            (15, 5, 6), // b15_bar1_stave2_time_sig_numerator between v5_bar1_time_sig_start and v6_bar1_time_sig_end
            (16, 5, 6), // b16_bar1_stave2_time_sig_denominator between v5_bar1_time_sig_start and v6_bar1_time_sig_end
            (17, 7, 8), // b17_bar1_voice1_notehead1 between v7_bar1_note1_start and v8_bar1_note1_end
            (18, 9, 10), // b18_bar1_voice1_notehead2 between v9_bar1_note2_start and v10_bar1_note2_end
            (19, 7, 8), // b19_bar1_voice2_notehead between v7_bar1_note1_start and v8_bar1_note1_end
            (20, 11, 12), // b20_bar1_barline between v11_bar1_barline_start and v12_bar1_barline_end
            (21, 13, 14), // b21_bar2_voice1_notehead1 between v13_bar2_note1_start and v14_bar2_note1_end
            (22, 15, 16), // b22_bar2_voice1_notehead2 between v15_bar2_note2_start and v16_bar2_note2_end
            (23, 13, 14), // b23_bar1_voice2_notehead2 between v13_bar2_note1_start and v14_bar2_note1_end
            (24, 17, 18), // b24_bar2_barline between v17_bar2_barline_start and v18_bar2_barline_end
            (25, 7, 8),   // b25_bar1_lyric1 between v7_bar1_note1_start and v8_bar1_note1_end
            (26, 9, 10),  // b26_bar1_lyric2 between v9_bar1_note2_start and v10_bar1_note2_end
            (27, 13, 14), // b27_bar2_lyric1 between v13_bar2_note1_start and v14_bar2_note1_end
            (28, 15, 16), // b28_bar2_lyric2 between v15_bar2_note2_start and v16_bar2_note2_end
        ];

        for (block_index, grid_line_before, grid_line_after) in blocks_in_columns {
            assert!(
                unwrap_block_start(&solution, block_index)
                    >= unwrap_v_line(&solution, grid_line_before)
            );
            assert!(
                unwrap_block_end(&solution, block_index)
                    <= unwrap_v_line(&solution, grid_line_after)
            );
        }

        // Check that column separation gaps and simulated rhythmic padding spaces
        // after noteheads have been correctly applied.

        let block_end_next_v_line_separation = vec![
            // The expected minimum distance between the end of a block and the start of the
            // given column. Tuple is (block index, grid line index, expected minimum separation)
            (11, 5, column_separation),
            (12, 5, column_separation),
            (13, 7, column_separation),
            (14, 7, column_separation),
            (15, 7, column_separation),
            (16, 7, column_separation),
            (17, 9, rhythmic_space_separation),
            (18, 11, rhythmic_space_separation),
            (19, 9, rhythmic_space_separation),
            (20, 13, column_separation),
            (21, 15, rhythmic_space_separation),
            (22, 17, rhythmic_space_separation),
            (23, 15, rhythmic_space_separation),
            (25, 9, column_separation),
            (26, 11, column_separation),
            (27, 15, column_separation),
            (28, 17, column_separation),
        ];

        for (block_index, grid_line_after, minimum_separation) in block_end_next_v_line_separation {
            assert!(
                unwrap_block_end(&solution, block_index) + minimum_separation
                    <= unwrap_v_line(&solution, grid_line_after)
            );
        }

        // Finally, check that lyric syllables are correctly centered beneath their
        // respective noteheads. (In a real score, syllables are not necessarily
        // centered - it usually depends on whether the syllable starts a new word
        // or not - but in this test we simply centered all syllables.)

        let syllables_underneath_noteheads = vec![
            // Tuple is (syllable block index, notehead block index, notehead center grid line index)
            (25, 17, 19),
            (26, 18, 20),
            (27, 21, 21),
            (28, 22, 22),
        ];

        for (syllable_block, notehead_block, notehead_center_grid_line) in
            syllables_underneath_noteheads
        {
            assert_eq!(
                unwrap_block_start(&solution, syllable_block)
                    + (unwrap_block_end(&solution, syllable_block)
                        - unwrap_block_start(&solution, syllable_block))
                        / 2.0,
                unwrap_v_line(&solution, notehead_center_grid_line)
            );

            assert_eq!(
                unwrap_block_start(&solution, notehead_block)
                    + (unwrap_block_end(&solution, notehead_block)
                        - unwrap_block_start(&solution, notehead_block))
                        / 2.0,
                unwrap_v_line(&solution, notehead_center_grid_line)
            );
        }
    }

    fn create_staveline_block(
        staveline: HorizontalGridLineIndex,
        systemic_line: VerticalGridLineIndex,
        system_end: VerticalGridLineIndex,
    ) -> LineBlock {
        let mut block = LineBlock::new_horizontal(
            None,
            Some(TICKS_ZERO),
            None,
            0.25.as_stave_spaces(),
            Color::BLACK,
            StrokeStyle::Solid,
            BlockLayer::Foreground,
        );

        block.lock_start_to_grid_line(systemic_line);
        block.lock_end_to_grid_line(system_end);
        block.lock_vertical_center_to_grid_line(staveline);

        block
    }

    fn create_glyph_block_on_staveline(
        staveline: HorizontalGridLineIndex,
        column_start: VerticalGridLineIndex,
        column_end: VerticalGridLineIndex,
        onset: Ticks,
        font: &impl SmuflFont,
        glyph: Glyph,
    ) -> GlyphBlock {
        let mut block = GlyphBlock::new(
            None,
            Some(onset),
            None,
            font,
            Color::BLACK,
            glyph,
            BlockLayer::Foreground,
        );

        block.lock_vertical_center_to_grid_line(staveline);
        block.float_horizontally_between_grid_lines(column_start, column_end);
        // TODO: AJRC - 22/8/21 - it's tempting to use start_align_between_grid_lines()
        // on the notehead, but this sets an EQ(STRONG) constraint on the notehead
        // position that conflicts with the center point of a wide lyric. Only
        // by floating the notehead between grid lines can we allow the
        // width of a wide lyric to "win" and push the center of the notehead
        // sideways. If we use start_align, then the notehead won't budge; the lyric
        // instead moves, and invariably collides with the lyric in the previous
        // notehead column. This could indicate that we need to weaken the
        // EQ() constraint on start_align. Perhaps if it was EQ(MEDIUM) instead of
        // EQ(STRONG), there'd be less of a problem using start_align. Or we
        // could allow the block constraint to actually take a strength parameter
        // when we define it, rather than trying to assign strengths to constraints
        // as part of LayoutSystem.engrave().

        block
    }

    fn create_barline_block(
        system_top: HorizontalGridLineIndex,
        system_bottom: HorizontalGridLineIndex,
        barline_column_start: VerticalGridLineIndex,
        barline_column_end: VerticalGridLineIndex,
        onset: Ticks,
    ) -> LineBlock {
        let mut block = LineBlock::new_vertical(
            None,
            Some(onset),
            None,
            0.5.as_stave_spaces(),
            Color::BLACK,
            StrokeStyle::Solid,
            BlockLayer::Foreground,
        );

        block.lock_top_to_grid_line(system_top);
        block.lock_bottom_to_grid_line(system_bottom);
        block.lock_start_between_grid_lines(
            barline_column_start,
            barline_column_end,
            STAVE_SPACES_ZERO,
        );

        block
    }

    fn create_systemic_line_block(
        system_top: HorizontalGridLineIndex,
        system_bottom: HorizontalGridLineIndex,
        systemic_line: VerticalGridLineIndex,
    ) -> LineBlock {
        let mut block = LineBlock::new_vertical(
            None,
            Some(TICKS_ZERO),
            None,
            0.25.as_stave_spaces(),
            Color::BLACK,
            StrokeStyle::Solid,
            BlockLayer::Foreground,
        );

        block.lock_top_to_grid_line(system_top);
        block.lock_bottom_to_grid_line(system_bottom);
        block.lock_horizontal_center_to_grid_line(systemic_line);

        block
    }

    fn create_lyric_underlay_block(
        lyric_underlay_top: HorizontalGridLineIndex,
        lyric_underlay_bottom: HorizontalGridLineIndex,
        notehead_start: VerticalGridLineIndex,
        notehead_center: VerticalGridLineIndex,
        notehead_end: VerticalGridLineIndex,
        lyric: &str,
    ) -> MarkupBlock {
        // We simulate the width for this test by assuming 0.5 stave spaces per character.

        let lyric_width = StaveSpaces::new(lyric.len() as f32 * 0.5);

        let lyric_height = 1.as_stave_spaces();

        let mut block = MarkupBlock::new(
            None,
            None,
            None,
            vec![MarkedUpLine::new(
                STAVE_SPACES_ZERO,
                STAVE_SPACES_ZERO,
                STAVE_SPACES_ZERO,
                STAVE_SPACES_ZERO,
                lyric_width,
                lyric_height,
                vec![],
                LineLayout::LineStartAligned,
                Border::none(),
            )],
            BlockLayer::Foreground,
            Some(lyric_width),
            Some(lyric_height),
        );

        block.lock_top_to_grid_line(lyric_underlay_top);
        block.lock_bottom_to_grid_line(lyric_underlay_bottom);
        block.float_horizontally_between_grid_lines(notehead_start, notehead_end);
        block.lock_horizontal_center_to_grid_line(notehead_center);

        block
    }

    fn unwrap_h_line(
        solution: &Result<EngravedSystem, EngravingError>,
        index: HorizontalGridLineIndex,
    ) -> StaveSpaces {
        assert!(solution.is_ok());

        let result = solution
            .as_ref()
            .unwrap()
            .get_horizontal_grid_line_positions()
            .get(index);

        assert!(result.is_some());

        *result.unwrap()
    }

    fn unwrap_v_line(
        solution: &Result<EngravedSystem, EngravingError>,
        index: VerticalGridLineIndex,
    ) -> StaveSpaces {
        assert!(solution.is_ok());

        let result = solution
            .as_ref()
            .unwrap()
            .get_vertical_grid_line_positions()
            .get(index);

        assert!(result.is_some());

        *result.unwrap()
    }

    fn unwrap_block_top(
        solution: &Result<EngravedSystem, EngravingError>,
        index: BlockIndex,
    ) -> StaveSpaces {
        assert!(solution.is_ok());

        let result = solution.as_ref().unwrap().get_foreground().get(index);

        assert!(result.is_some());

        result.unwrap().get_y()
    }

    fn unwrap_block_start(
        solution: &Result<EngravedSystem, EngravingError>,
        index: BlockIndex,
    ) -> StaveSpaces {
        assert!(solution.is_ok());

        let result = solution.as_ref().unwrap().get_foreground().get(index);

        assert!(result.is_some());

        result.unwrap().get_x()
    }

    fn unwrap_block_end(
        solution: &Result<EngravedSystem, EngravingError>,
        index: BlockIndex,
    ) -> StaveSpaces {
        assert!(solution.is_ok());

        let result = solution.as_ref().unwrap().get_foreground().get(index);

        assert!(result.is_some());

        result.unwrap().get_x() + result.unwrap().get_width()
    }

    fn unwrap_block_bottom(
        solution: &Result<EngravedSystem, EngravingError>,
        index: BlockIndex,
    ) -> StaveSpaces {
        assert!(solution.is_ok());

        let result = solution.as_ref().unwrap().get_foreground().get(index);

        assert!(result.is_some());

        result.unwrap().get_y() + result.unwrap().get_height()
    }

    #[test]
    fn test_system_start_align() {
        let solution = create_justification_test(SystemJustification::AlignStart).engrave();

        assert!(solution.is_ok());

        let solution = solution.unwrap();

        // Start alignment should have a leading edge at 0.0 and, for this
        // justification test, a trailing edge at 15.0. The fact that the test
        // asks for a target system width of 30.0 is irrelevant when the
        // system justification is set to start alignment.

        assert_eq!(
            solution.get_vertical_grid_line_positions().get(0).unwrap(),
            0.as_stave_spaces()
        );
        assert_eq!(
            solution.get_vertical_grid_line_positions().get(1).unwrap(),
            15.as_stave_spaces()
        );
    }

    #[test]
    fn test_system_end_align() {
        let solution = create_justification_test(SystemJustification::AlignEnd).engrave();

        assert!(solution.is_ok());

        let solution = solution.unwrap();

        // The justification test scenario has a total width of 15 stave spaces
        // and sets a target system width of 30 stave spaces, so end alignment
        // should have a leading edge at 15 and a trailing edge at 30.

        assert_eq!(
            solution.get_vertical_grid_line_positions().get(0).unwrap(),
            15.as_stave_spaces()
        );
        assert_eq!(
            solution.get_vertical_grid_line_positions().get(1).unwrap(),
            30.as_stave_spaces()
        );
    }

    #[test]
    fn test_system_center_align() {
        let solution = create_justification_test(SystemJustification::Centered).engrave();

        assert!(solution.is_ok());

        let solution = solution.unwrap();

        // The justification test scenario has a total width of 15 stave spaces
        // and sets a target system width of 30 stave spaces, so center alignment
        // should have a leading edge at (30 - 15) / 2 = 7.5 and a trailing edge
        // at 7.5 + 15 = 22.5.

        assert_eq!(
            solution.get_vertical_grid_line_positions().get(0).unwrap(),
            7.5.as_stave_spaces()
        );
        assert_eq!(
            solution.get_vertical_grid_line_positions().get(1).unwrap(),
            22.5.as_stave_spaces()
        );
    }

    #[test]
    fn test_system_justify() {
        let solution = create_justification_test(SystemJustification::Justified).engrave();

        assert!(solution.is_ok());

        let solution = solution.unwrap();

        // The justification test scenario has a total width of 15 stave spaces.
        // There are three notehead glyphs, each followed by a spacing block.
        // Justifying the test out from 15 stave spaces to 30 stave spaces
        // means we expect each spacing block to take on (30 - 15) / 3 additional
        // stave spaces of padding. The spacing blocks themselves are filtered out
        // when blocks are converted to engravable, so we only have the positions
        // of the glyphs available to examine. Before justification, the spacing
        // blocks ensured that the glyphs appeared at (0,0), (5,0) and (10,0); adding
        // (30 - 15) / 3 = 5 additional stave spaces of padding to each spacing
        // block should result in the two glyphs now appearing at (0+5*0,0),
        // (5+5*1,0) and (10+5*2,0) = (0,0), (10,0) and (20,0) in the engraving.

        // Because the simulated staveline is in the background layer, and the
        // spacing blocks are filtered out of the final engraving, we expect to find
        // the glyph engravable at index positions 0, 1, and 2 in the foreground layer.

        assert_eq!(
            solution.get_foreground().get(0).unwrap().get_x(),
            STAVE_SPACES_ZERO
        );
        assert_eq!(
            solution.get_foreground().get(1).unwrap().get_x(),
            10.as_stave_spaces()
        );
        assert_eq!(
            solution.get_foreground().get(2).unwrap().get_x(),
            20.as_stave_spaces()
        );

        // In addition to the glyph blocks moving, we also expect to see the system end
        // vertical grid line at index 1 expand its position to 30 stave spaces.

        assert_eq!(
            solution.get_vertical_grid_line_positions().get(1).unwrap(),
            30.as_stave_spaces()
        );
    }

    fn create_justification_test(justification: SystemJustification) -> LayoutSystem {
        // A simple set of blocks and constraints that let us play with
        // justification settings.

        // We align six blocks on a single horizontal grid line: a glyph, a spacer,
        // a glyph, a spacer, a glyph, and a spacer. The total width will be
        // 15 stave spaces. We ask for a target system width double that,
        // so the effects of system alignment are clear.

        let font = Bravura::new();

        let h0 = HorizontalGridLine::new(HorizontalGridLineType::Staveline1);

        let v0 = VerticalGridLine::new(0, VerticalGridLineType::SystemStart);

        let mut v1 = VerticalGridLine::new(0, VerticalGridLineType::SystemEnd);

        let mut b0 = LineBlock::new(
            None,
            None,
            None,
            0.25.as_stave_spaces(),
            Color::BLACK,
            StrokeStyle::Solid,
            BlockLayer::Background,
        );

        b0.lock_vertical_center_to_grid_line(0);
        b0.lock_start_to_grid_line(0);
        b0.lock_end_to_grid_line(1);

        let mut b1 = GlyphBlock::new(
            None,
            Some(TICKS_ZERO),
            None,
            &font,
            Color::BLACK,
            Glyph::NoteheadBlack,
            BlockLayer::Foreground,
        );

        let notehead_width = b1.get_fixed_width();

        let mut v2 =
            VerticalGridLine::new(1, VerticalGridLineType::NoteheadLine0NoteheadStackStart);

        v2.lock_to_grid_line(0);

        let v3 = VerticalGridLine::new(1, VerticalGridLineType::NoteheadLine0NoteheadStackStart);

        b1.float_horizontally_between_grid_lines(2, 3);

        let mut v4 = VerticalGridLine::new(1, VerticalGridLineType::RhythmicSpacingStart);

        v4.lock_to_grid_line(3);

        let mut b2 = SpacingBlock::new(5.as_stave_spaces() - notehead_width);

        let v5 = VerticalGridLine::new(1, VerticalGridLineType::RhythmicSpacingEnd);

        b2.float_horizontally_between_grid_lines(4, 5);

        let mut v6 =
            VerticalGridLine::new(2, VerticalGridLineType::NoteheadLine0NoteheadStackStart);

        v6.lock_to_grid_line(5);

        let mut b3 = GlyphBlock::new(
            None,
            Some(NotatedDuration::Crotchet.as_ticks()),
            None,
            &font,
            Color::BLACK,
            Glyph::NoteheadBlack,
            BlockLayer::Foreground,
        );

        let v7 = VerticalGridLine::new(2, VerticalGridLineType::NoteheadLine0NoteheadStackStart);

        b3.float_horizontally_between_grid_lines(6, 7);

        let mut v8 = VerticalGridLine::new(2, VerticalGridLineType::RhythmicSpacingStart);

        v8.lock_to_grid_line(7);

        let mut b4 = SpacingBlock::new(5.as_stave_spaces() - notehead_width);

        let v9 = VerticalGridLine::new(2, VerticalGridLineType::RhythmicSpacingEnd);

        b4.float_horizontally_between_grid_lines(8, 9);

        let mut v10 =
            VerticalGridLine::new(3, VerticalGridLineType::NoteheadLine0NoteheadStackStart);

        v10.lock_to_grid_line(9);

        let mut b5 = GlyphBlock::new(
            None,
            Some(NotatedDuration::Minim.as_ticks()),
            None,
            &font,
            Color::BLACK,
            Glyph::NoteheadBlack,
            BlockLayer::Foreground,
        );

        let v11 = VerticalGridLine::new(3, VerticalGridLineType::NoteheadLine0NoteheadStackStart);

        b5.float_horizontally_between_grid_lines(10, 11);

        let mut v12 = VerticalGridLine::new(3, VerticalGridLineType::RhythmicSpacingStart);

        v12.lock_to_grid_line(11);

        let mut b6 = SpacingBlock::new(5.as_stave_spaces() - notehead_width);

        let v13 = VerticalGridLine::new(3, VerticalGridLineType::RhythmicSpacingEnd);

        b6.float_horizontally_between_grid_lines(12, 13);

        v1.lock_to_grid_line(13);

        LayoutSystem::new(
            0,
            0.as_ticks(),
            0.as_ticks(),
            justification,
            30.as_stave_spaces(),
            vec![h0],
            vec![v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11, v12, v13],
            0,
            0,
            vec![
                b0.into(),
                b1.into(),
                b2.into(),
                b3.into(),
                b4.into(),
                b5.into(),
                b6.into(),
            ],
            false,
            false,
            false,
            false,
        )
    }
}
