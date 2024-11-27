use std::{thread, time::Duration};

use opencv::{core::{in_range, Point, VecN, Vector}, highgui::{imshow, named_window, WINDOW_AUTOSIZE}, imgproc::{bounding_rect, contour_area, cvt_color, find_contours, rectangle, CHAIN_APPROX_SIMPLE, COLOR_BGR2HSV, LINE_8, RETR_EXTERNAL}, prelude::*, videoio::{VideoCapture, CAP_ANY}};

const SIZE_THRESHOLD: i32 = 300;

#[derive(Debug, Copy, Clone, PartialEq)]
enum Colour {
    Red,
    Blue,
}

#[derive(Debug, Copy, Clone, PartialEq)]
enum Size {
    TwoBy2,
    TwoBy4,
}

fn main() {
    let mut capture = VideoCapture::new(0, CAP_ANY).expect("could not get capture!");
    let mut frame_in = Mat::default();
    let mut frame_hsv = Mat::default();
    named_window("shmeep", WINDOW_AUTOSIZE).expect("could not create preview window!");

    loop {
	capture.read(&mut frame_in).expect("Video frame capture failed!");
	cvt_color(&mut frame_in, &mut frame_hsv, COLOR_BGR2HSV, 0).expect("Could not convert image to HSV space!");
	
	let mut mask_blue = Mat::default();
	let mut mask_red = Mat::default();

	in_range(&frame_hsv, &[90, 100, 100], &[140, 255, 255], &mut mask_blue).expect("Could not create blue mask!");
	in_range(&frame_hsv, &[0, 100, 100], &[10, 255, 255], &mut mask_red).expect("Could not create red mask!");

	let mut blue_contours: Vector<Vector<Point>> = Vector::new();
	let mut red_contours: Vector<Vector<Point>> = Vector::new();
	
	find_contours(&mask_blue, &mut blue_contours, RETR_EXTERNAL, CHAIN_APPROX_SIMPLE, Point::new(0, 0)).expect("Could not find blue contours!");
	find_contours(&mask_red, &mut red_contours, RETR_EXTERNAL, CHAIN_APPROX_SIMPLE, Point::new(0, 0)).expect("Could not find red contours!");

	let mut rightmost_red_contour: Vector<Point> = find_rightmost_contour(&red_contours, 200);
	let mut rightmost_blue_contour: Vector<Point> = find_rightmost_contour(&blue_contours, 200);
	let blue_rect = bounding_rect(&rightmost_blue_contour).expect("could not create a bounding box for blue contour!");
	let red_rect = bounding_rect(&rightmost_red_contour).expect("could not create a bounding box for red contour!");

	let (rightmost_contour, rightmost_rect, colour) = if blue_rect.x > red_rect.x {
	    (rightmost_blue_contour, blue_rect, Colour::Blue)
	} else {
	    (rightmost_red_contour, red_rect, Colour::Red)
	};

	if rightmost_rect.x < (540 - (rightmost_rect.width / 2)) {
	    //not at assumed dropoff point yet, we shall wait.
	    rectangle(&mut frame_in, rightmost_rect, VecN::from_array([255., 0., 0., 255.]), 1, LINE_8, 0).expect("could not draw preview rectangle!");
	    imshow("shmeep", &frame_in).expect("could not preview image!");

	    continue;
	} else {
	    	    rectangle(&mut frame_in, rightmost_rect, VecN::from_array([0., 255., 0., 255.]), 1, LINE_8, 0).expect("could not draw preview rectangle!");
	    imshow("shmeep", &frame_in).expect("could not preview image!");
	}

	let size = if rightmost_rect.width > SIZE_THRESHOLD {
	    Size::TwoBy2
	} else {
	    Size::TwoBy4
	};

	sort(colour, size);
	wait_for_block_drop();
	unsort(colour, size);
    }
}

fn sort(colour: Colour, size: Size) {
    //stub. this will have to activate the right gpio pin to control the servo.
}

fn wait_for_block_drop() {
    //also a stub. this will have to wait for a signal from the photoresistor
    thread::sleep(Duration::from_secs(2));
}

fn unsort(colour: Colour, size: Size) {
    //one more stub. this will simply close the servo that was previously opened.
}

fn find_rightmost_contour(contours: &Vector<Vector<Point>>, min_area: i32) -> Vector<Point> {
    let mut rightmost_contour = Vector::new();
    for contour in contours {
	let area = contour_area(&contour, false).expect("could not calculate contour area!") as i32;
	if area < min_area {
	    continue;
	}
	let current = bounding_rect(&rightmost_contour).expect("Could not create a bounding rect for rightest found contour!");
	let maybe_new = bounding_rect(&contour).expect("Could not create a bounding rect for newest contour!");
	if current.x < maybe_new.x {
	    rightmost_contour = contour;
	}
    }
    rightmost_contour
}
