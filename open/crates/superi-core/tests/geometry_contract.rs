use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::geometry::{AspectRatio, Matrix3, PixelBounds, Point2, Rect, Vector2};

#[test]
fn points_and_vectors_retain_distinct_finite_values() {
    let point = Point2::new(12.5, -3.25).unwrap();
    let vector = Vector2::new(-2.0, 7.5).unwrap();

    assert_eq!((point.x(), point.y()), (12.5, -3.25));
    assert_eq!((vector.x(), vector.y()), (-2.0, 7.5));
    assert_eq!(Point2::ORIGIN, Point2::new(0.0, 0.0).unwrap());
    assert_eq!(Vector2::ZERO, Vector2::new(0.0, 0.0).unwrap());
}

#[test]
fn point_and_vector_operations_preserve_geometric_meaning() {
    let start = Point2::new(2.0, 3.0).unwrap();
    let displacement = Vector2::new(4.0, -5.0).unwrap();
    let end = start.checked_offset(displacement).unwrap();

    assert_eq!(end, Point2::new(6.0, -2.0).unwrap());
    assert_eq!(start.checked_vector_to(end).unwrap(), displacement);
    assert_eq!(
        displacement
            .checked_add(Vector2::new(1.0, 2.0).unwrap())
            .unwrap(),
        Vector2::new(5.0, -3.0).unwrap()
    );
    assert_eq!(
        displacement.checked_scale(0.5).unwrap(),
        Vector2::new(2.0, -2.5).unwrap()
    );
}

#[test]
fn non_finite_geometry_uses_the_shared_actionable_error_contract() {
    for error in [
        Point2::new(f64::NAN, 0.0).unwrap_err(),
        Point2::new(0.0, f64::INFINITY).unwrap_err(),
        Vector2::new(f64::NEG_INFINITY, 0.0).unwrap_err(),
        Vector2::new(0.0, f64::NAN).unwrap_err(),
    ] {
        assert_eq!(error.category(), ErrorCategory::InvalidInput);
        assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
        assert_eq!(error.contexts()[0].component(), "superi-core.geometry");
    }
}

#[test]
fn checked_geometry_operations_reject_floating_point_overflow() {
    let large = Vector2::new(f64::MAX, f64::MAX).unwrap();
    let error = large.checked_scale(2.0).unwrap_err();

    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert!(error.message().contains("finite"));
    assert_eq!(error.contexts()[0].operation(), "scale_vector");
}

#[test]
fn rectangles_are_normalized_half_open_spatial_extents() {
    let bounds = Rect::new(
        Point2::new(-2.0, 3.0).unwrap(),
        Point2::new(8.0, 9.0).unwrap(),
    )
    .unwrap();

    assert_eq!(bounds.width(), 10.0);
    assert_eq!(bounds.height(), 6.0);
    assert!(!bounds.is_empty());
    assert!(bounds.contains(Point2::new(-2.0, 3.0).unwrap()));
    assert!(bounds.contains(Point2::new(7.999, 8.999).unwrap()));
    assert!(!bounds.contains(Point2::new(8.0, 8.0).unwrap()));
    assert!(!bounds.contains(Point2::new(0.0, 9.0).unwrap()));

    let empty = Rect::new(Point2::ORIGIN, Point2::ORIGIN).unwrap();
    assert!(empty.is_empty());
    assert!(!empty.contains(Point2::ORIGIN));
}

#[test]
fn rectangle_intersection_and_union_preserve_extent() {
    let left = Rect::new(
        Point2::new(0.0, 0.0).unwrap(),
        Point2::new(10.0, 8.0).unwrap(),
    )
    .unwrap();
    let right = Rect::new(
        Point2::new(6.0, -2.0).unwrap(),
        Point2::new(14.0, 4.0).unwrap(),
    )
    .unwrap();

    assert_eq!(
        left.intersection(right).unwrap(),
        Rect::new(
            Point2::new(6.0, 0.0).unwrap(),
            Point2::new(10.0, 4.0).unwrap()
        )
        .unwrap()
    );
    assert_eq!(
        left.checked_union(right).unwrap(),
        Rect::new(
            Point2::new(0.0, -2.0).unwrap(),
            Point2::new(14.0, 8.0).unwrap()
        )
        .unwrap()
    );

    let touching = Rect::new(
        Point2::new(10.0, 0.0).unwrap(),
        Point2::new(12.0, 2.0).unwrap(),
    )
    .unwrap();
    assert_eq!(left.intersection(touching), None);
    assert!(Rect::new(
        Point2::new(-f64::MAX, 0.0).unwrap(),
        Point2::new(f64::MAX, 1.0).unwrap()
    )
    .is_err());
}

#[test]
fn invalid_rectangles_are_user_correctable() {
    let error = Rect::new(
        Point2::new(2.0, 0.0).unwrap(),
        Point2::new(1.0, 4.0).unwrap(),
    )
    .unwrap_err();

    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(error.contexts()[0].operation(), "create_rectangle");
}

#[test]
fn aspect_ratios_are_positive_exact_and_canonical() {
    let cinema = AspectRatio::new(4096, 2160).unwrap();
    assert_eq!(cinema, AspectRatio::new(256, 135).unwrap());
    assert_eq!((cinema.numerator(), cinema.denominator()), (256, 135));
    assert_eq!(cinema.to_string(), "256:135");

    for error in [
        AspectRatio::new(0, 1080).unwrap_err(),
        AspectRatio::new(1920, 0).unwrap_err(),
    ] {
        assert_eq!(error.category(), ErrorCategory::InvalidInput);
        assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    }
}

#[test]
fn pixel_bounds_use_signed_half_open_edges_and_exact_sizes() {
    let bounds = PixelBounds::from_origin_size(-4, 3, 12, 7).unwrap();

    assert_eq!((bounds.min_x(), bounds.min_y()), (-4, 3));
    assert_eq!((bounds.max_x(), bounds.max_y()), (8, 10));
    assert_eq!((bounds.width(), bounds.height()), (12, 7));
    assert!(bounds.contains(-4, 3));
    assert!(bounds.contains(7, 9));
    assert!(!bounds.contains(8, 9));
    assert!(!bounds.contains(7, 10));
    assert_eq!(
        bounds.aspect_ratio().unwrap(),
        AspectRatio::new(12, 7).unwrap()
    );
    assert_eq!(
        bounds.to_rect(),
        Rect::new(
            Point2::new(-4.0, 3.0).unwrap(),
            Point2::new(8.0, 10.0).unwrap()
        )
        .unwrap()
    );
}

#[test]
fn pixel_bound_operations_are_checked_and_predictable() {
    let source = PixelBounds::new(-5, -3, 5, 7).unwrap();
    let clip = PixelBounds::new(0, -8, 12, 2).unwrap();

    assert_eq!(
        source.intersection(clip).unwrap(),
        PixelBounds::new(0, -3, 5, 2).unwrap()
    );
    assert_eq!(source.union(clip), PixelBounds::new(-5, -8, 12, 7).unwrap());
    assert_eq!(
        source.checked_translate(10, -2).unwrap(),
        PixelBounds::new(5, -5, 15, 5).unwrap()
    );

    assert!(PixelBounds::new(5, 0, 4, 1).is_err());
    assert!(PixelBounds::from_origin_size(i32::MAX, 0, 1, 1).is_err());
    assert!(source.checked_translate(i32::MAX, 0).is_err());
    assert!(PixelBounds::new(0, 0, 0, 0)
        .unwrap()
        .aspect_ratio()
        .is_err());
}

#[test]
fn matrices_have_explicit_row_major_column_vector_semantics() {
    let rows = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [0.0, 0.0, 1.0]];
    let matrix = Matrix3::from_rows(rows).unwrap();

    assert_eq!(matrix.rows(), rows);
    assert_eq!(
        Matrix3::IDENTITY.rows(),
        [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
    );
    assert_eq!(
        matrix
            .checked_transform_point(Point2::new(2.0, 3.0).unwrap())
            .unwrap(),
        Point2::new(11.0, 29.0).unwrap()
    );
}

#[test]
fn matrix_composition_names_the_application_order() {
    let scale = Matrix3::scale(Vector2::new(2.0, 3.0).unwrap());
    let translation = Matrix3::translation(Vector2::new(10.0, -4.0).unwrap());
    let scale_then_translate = scale.checked_then(translation).unwrap();
    let point = Point2::new(1.0, 2.0).unwrap();

    assert_eq!(
        scale_then_translate.checked_transform_point(point).unwrap(),
        Point2::new(12.0, 2.0).unwrap()
    );
    assert_eq!(
        scale_then_translate,
        translation.checked_mul(scale).unwrap()
    );
    assert_ne!(
        scale_then_translate,
        translation.checked_then(scale).unwrap()
    );
}

#[test]
fn homogeneous_point_mapping_is_checked() {
    let perspective =
        Matrix3::from_rows([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.5, 0.0, 1.0]]).unwrap();
    assert_eq!(
        perspective
            .checked_transform_point(Point2::new(2.0, 4.0).unwrap())
            .unwrap(),
        Point2::new(1.0, 2.0).unwrap()
    );

    let horizon = Matrix3::from_rows([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]]).unwrap();
    let error = horizon
        .checked_transform_point(Point2::new(0.0, 1.0).unwrap())
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.contexts()[0].operation(), "transform_point");
}

#[test]
fn matrix_inverse_round_trips_and_rejects_singular_values() {
    let transform = Matrix3::scale(Vector2::new(4.0, -2.0).unwrap())
        .checked_then(Matrix3::translation(Vector2::new(13.0, 7.0).unwrap()))
        .unwrap();
    let inverse = transform.checked_inverse().unwrap();
    let source = Point2::new(-3.0, 8.0).unwrap();
    let mapped = transform.checked_transform_point(source).unwrap();

    assert_close_point(inverse.checked_transform_point(mapped).unwrap(), source);
    assert_eq!(
        Matrix3::scale(Vector2::new(0.0, 1.0).unwrap())
            .checked_inverse()
            .unwrap_err()
            .contexts()[0]
            .operation(),
        "invert_matrix"
    );
    assert!(Matrix3::from_rows([[f64::NAN, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]).is_err());
}

#[test]
fn transformed_rectangle_bounds_include_all_four_corners() {
    let rect = Rect::new(
        Point2::new(1.0, 2.0).unwrap(),
        Point2::new(5.0, 8.0).unwrap(),
    )
    .unwrap();
    let transform =
        Matrix3::from_rows([[0.0, -1.0, 10.0], [1.0, 0.0, 20.0], [0.0, 0.0, 1.0]]).unwrap();

    assert_eq!(
        rect.checked_transform_bounds(transform).unwrap(),
        Rect::new(
            Point2::new(2.0, 21.0).unwrap(),
            Point2::new(8.0, 25.0).unwrap()
        )
        .unwrap()
    );

    let crossing_horizon =
        Matrix3::from_rows([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 0.0, -3.0]]).unwrap();
    assert!(rect.checked_transform_bounds(crossing_horizon).is_err());
}

fn assert_close_point(actual: Point2, expected: Point2) {
    assert!((actual.x() - expected.x()).abs() <= 1.0e-12);
    assert!((actual.y() - expected.y()).abs() <= 1.0e-12);
}
