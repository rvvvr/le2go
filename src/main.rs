use std::{sync::mpsc, thread, time::Duration};

use libcamera::{camera::CameraConfigurationStatus, camera_manager::CameraManager, framebuffer_allocator::{FrameBuffer, FrameBufferAllocator}, framebuffer_map::MemoryMappedFrameBuffer, pixel_format::PixelFormat, request::{Request, ReuseFlag}, stream::StreamRole};
use opencv::{boxed_ref::BoxedRef, core::{in_range, merge, Point, VecN, Vector}, highgui::{imshow, named_window, wait_key, WINDOW_AUTOSIZE}, imgcodecs::{imdecode_to, imwrite, IMREAD_COLOR, IMREAD_GRAYSCALE}, imgproc::{bounding_rect, contour_area, cvt_color, find_contours, rectangle, CHAIN_APPROX_SIMPLE, COLOR_BGR2HSV, LINE_8, RETR_EXTERNAL}, prelude::*};

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
    named_window("shmeep", WINDOW_AUTOSIZE).expect("Could not create window!");
    let camera_manager = CameraManager::new().expect("Could not create camera manager!");
    let cameras = camera_manager.cameras();
    let camera = cameras.get(0).expect("Could not get camera!");
    let mut capture = camera.acquire().expect("Could not activate camera!");
    let mut config = capture.generate_configuration(&[StreamRole::VideoRecording]).expect("Could not generate camera configs!");

    let formats = config.get(0).unwrap().formats().pixel_formats();
    for i in (0..formats.len()) {
	let format = formats.get(i).unwrap();
	println!("{:?}: {:#08x}", format, format.fourcc());
    }
    
    let mut cfg = config.get_mut(0).unwrap();
    cfg.set_pixel_format(PixelFormat::new(0x34324752, 0));
    cfg.set_size(libcamera::geometry::Size { width: 640, height: 480 });

    match config.validate() {
        CameraConfigurationStatus::Valid => println!("Camera configuration valid!"),
        CameraConfigurationStatus::Adjusted => println!("Camera configuration was adjusted: {:#?}", config),
        CameraConfigurationStatus::Invalid => panic!("Error validating camera configuration"),
    }

    capture.configure(&mut config).expect("Could not configure camera!");
    
    let mut alloc = FrameBufferAllocator::new(&camera);

    let cfg = config.get(0).unwrap();
    let stream = cfg.stream().unwrap();
    let buffers = alloc.alloc(&stream).unwrap();
    println!("Allocated {} buffers", buffers.len());

    let buffers = buffers
        .into_iter()
        .map(|buf| MemoryMappedFrameBuffer::new(buf).unwrap())
        .collect::<Vec<_>>();

    let reqs = buffers
        .into_iter()
        .enumerate()
        .map(|(i, buf)| {
	    let mut req = capture.create_request(Some(i as u64)).unwrap();
	    req.add_buffer(&stream, buf).unwrap();
	    req
	})
        .collect::<Vec<_>>();
    
    let (tx, rx) = mpsc::channel();
    capture.on_request_completed(move |mut request: Request| {
	tx.send(request).unwrap();
    });

    capture.start(None).unwrap();

    for req in reqs {
	capture.queue_request(req).unwrap();
    }

    let mut frame_in = Mat::default();
    let mut frame_hsv = Mat::default();
    let mut mask_blue = Mat::default();
    let mut mask_red = Mat::default();
    
    loop {
	let mut req = rx.recv_timeout(Duration::from_secs(2)).expect("Camera request failed!");
	
	let framebuffer: &MemoryMappedFrameBuffer<FrameBuffer> = req.buffer(&stream).expect("Could not get framebuffer from request!");

	let mut planes = framebuffer.data();
	println!("n planes: {}", planes.len());
	let mut frame = planes.pop().unwrap().to_vec();
	
	let mut frame_in = Mat::new_rows_cols_with_bytes_mut::<VecN<u8, 3>>(480, 640, &mut frame).unwrap();

	cvt_color(&mut frame_in, &mut frame_hsv, COLOR_BGR2HSV, 0).expect("Could not convert image to HSV space!");

	in_range(&frame_hsv, &[90, 100, 100], &[140, 255, 255], &mut mask_blue).expect("Could not create blue mask!");
	in_range(&frame_hsv, &[0, 100, 100], &[10, 255, 255], &mut mask_red).expect("Could not create red mask!");

	let mut blue_contours: Vector<Vector<Point>> = Vector::new();
	let mut red_contours: Vector<Vector<Point>> = Vector::new();
	
	find_contours(&mask_blue, &mut blue_contours, RETR_EXTERNAL, CHAIN_APPROX_SIMPLE, Point::new(0, 0)).expect("Could not find blue contours!");
	find_contours(&mask_red, &mut red_contours, RETR_EXTERNAL, CHAIN_APPROX_SIMPLE, Point::new(0, 0)).expect("Could not find red contours!");

	let mut rightmost_red_contour: Vector<Point> = find_rightmost_contour(&red_contours, 1000);
	let mut rightmost_blue_contour: Vector<Point> = find_rightmost_contour(&blue_contours, 1000);
	let blue_rect = bounding_rect(&rightmost_blue_contour).expect("could not create a bounding box for blue contour!");
	let red_rect = bounding_rect(&rightmost_red_contour).expect("could not create a bounding box for red contour!");

	let (rightmost_contour, rightmost_rect, colour) = if blue_rect.x > red_rect.x {
	    (rightmost_blue_contour, blue_rect, Colour::Blue)
	} else {
	    (rightmost_red_contour, red_rect, Colour::Red)
	};

	if rightmost_rect.x < (240 - (rightmost_rect.width / 2)) {
	    //not at assumed dropoff point yet, we shall wait.
	    rectangle(&mut frame_in, rightmost_rect, VecN::from_array([255., 0., 0., 255.]), 1, LINE_8, 0).expect("could not draw preview rectangle!");
	    
	    imshow("shmeep", &frame_in).expect("could not preview image!");
	    
	    let key = wait_key(20).unwrap();
	    if key == 32 {
		break;
	    }
	    
	    req.reuse(ReuseFlag::REUSE_BUFFERS);
	    capture.queue_request(req).expect("Could not requeue request!");
	    
	    continue;
	} else {
	    rectangle(&mut frame_in, rightmost_rect, VecN::from_array([0., 255., 0., 255.]), 1, LINE_8, 0).expect("could not draw preview rectangle!");
	    
	    imshow("shmeep", &frame_in).expect("could not preview image!");
	    
	    let key = wait_key(20).unwrap();
	    if key == 32 {
		break;
	    }
	}

	let size = if rightmost_rect.width > SIZE_THRESHOLD {
	    Size::TwoBy2
	} else {
	    Size::TwoBy4
	};

/*	sort(colour, size);
	wait_for_block_drop();
	unsort(colour, size);*/

	req.reuse(ReuseFlag::REUSE_BUFFERS);
	capture.queue_request(req).expect("Could not requeue request!");
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
