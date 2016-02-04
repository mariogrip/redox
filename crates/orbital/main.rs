extern crate core;
extern crate system;

use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Write};
use std::mem::size_of;
use std::process::Command;
use std::thread;

use system::error::{Error, Result, EBADF};
use system::scheme::{Packet, Scheme};

pub use self::color::Color;
pub use self::display::Display;
pub use self::event::{Event, EventOption};
pub use self::font::Font;
pub use self::image::{Image, ImageRoi};
pub use self::window::Window;

use self::bmp::BmpFile;
use self::event::{EVENT_KEY, EVENT_MOUSE, QuitEvent};

pub mod bmp;
pub mod color;
pub mod display;
#[path="../../kernel/common/event.rs"]
pub mod event;
pub mod font;
pub mod image;
pub mod window;

struct OrbitalScheme {
    display: Display,
    cursor: Image,
    cursor_x: i32,
    cursor_y: i32,
    dragging: bool,
    drag_x: i32,
    drag_y: i32,
    next_id: isize,
    next_x: i32,
    next_y: i32,
    order: VecDeque<usize>,
    windows: BTreeMap<usize, Window>,
    redraw: bool,
}

impl OrbitalScheme {
    fn new(display: Display) -> OrbitalScheme {
        OrbitalScheme {
            display: display,
            cursor: BmpFile::from_path("/ui/cursor.bmp"),
            cursor_x: 0,
            cursor_y: 0,
            dragging: false,
            drag_x: 0,
            drag_y: 0,
            next_id: 1,
            next_x: 20,
            next_y: 20,
            order: VecDeque::new(),
            windows: BTreeMap::new(),
            redraw: true,
        }
    }

    fn event_loop(&mut self, socket: &mut File) {
        loop {
            for event in self.display.events() {
                if event.code == EVENT_KEY {
                    if let Some(id) = self.order.front() {
                        if let Some(mut window) = self.windows.get_mut(&id) {
                            window.event(event);
                        }
                    }
                } else if event.code == EVENT_MOUSE {
                    self.cursor_x = event.a as i32;
                    self.cursor_y = event.b as i32;
                    self.redraw = true;

                    if self.dragging {
                        if event.c > 0 {
                            if let Some(id) = self.order.front() {
                                if let Some(mut window) = self.windows.get_mut(&id) {
                                    window.x += self.cursor_x - self.drag_x;
                                    window.y += self.cursor_y - self.drag_y;
                                    self.drag_x = self.cursor_x;
                                    self.drag_y = self.cursor_y;
                                } else {
                                    self.dragging = false;
                                }
                            } else {
                                self.dragging = false;
                            }
                        } else {
                            self.dragging = false;
                        }
                    } else {
                        let mut focus = 0;
                        let mut i = 0;
                        for id in self.order.iter() {
                            if let Some(mut window) = self.windows.get_mut(&id) {
                                if window.contains(event.a as i32, event.b as i32) {
                                    let mut window_event = event;
                                    window_event.a -= window.x as i64;
                                    window_event.b -= window.y as i64;
                                    window.event(window_event);
                                    if event.c > 0 {
                                        focus = i;
                                    }
                                    break;
                                } else if window.title_contains(event.a as i32, event.b as i32) {
                                    if event.c > 0 {
                                        focus = i;
                                        if window.exit_contains(event.a as i32, event.b as i32) {
                                            window.event(QuitEvent.to_event());
                                        } else {
                                            self.dragging = true;
                                            self.drag_x = self.cursor_x;
                                            self.drag_y = self.cursor_y;
                                        }
                                    }
                                    break;
                                }
                            }
                            i += 1;
                        }
                        if focus > 0 {
                            if let Some(id) = self.order.remove(focus) {
                                self.order.push_front(id);
                            }
                        }
                    }
                }
            }

            let mut packet = Packet::default();
            while socket.read(&mut packet).unwrap() == size_of::<Packet>() {
                self.handle(&mut packet);
                socket.write(&packet).unwrap();
            }

            if self.redraw {
                self.redraw = false;
                self.display.as_roi().set(Color::rgb(75, 163, 253));

                let mut i = self.order.len();
                for id in self.order.iter().rev() {
                    i -= 1;
                    if let Some(mut window) = self.windows.get_mut(&id) {
                        window.draw(&mut self.display, i == 0);
                    }
                }

                self.display.roi(self.cursor_x, self.cursor_y, self.cursor.width(), self.cursor.height()).blend(&self.cursor.as_roi());
                self.display.flip();
            }

            thread::yield_now();
        }
    }
}

impl Scheme for OrbitalScheme {
    #[allow(unused_variables)]
    fn open(&mut self, path: &str, flags: usize, mode: usize) -> Result<usize> {
        let mut parts = path.split("/").skip(1);

        let mut x = parts.next().unwrap_or("").parse::<i32>().unwrap_or(0);
        let mut y = parts.next().unwrap_or("").parse::<i32>().unwrap_or(0);
        let width = parts.next().unwrap_or("").parse::<i32>().unwrap_or(0);
        let height = parts.next().unwrap_or("").parse::<i32>().unwrap_or(0);

        let mut title = parts.next().unwrap_or("").to_string();
        for part in parts {
            title.push('/');
            title.push_str(part);
        }

        let id = self.next_id as usize;
        self.next_id += 1;
        if self.next_id < 0 {
            self.next_id = 1;
        }

        if x < 0 && y < 0 {
            x = self.next_x;
            y = self.next_y;

            self.next_x += 20;
            if self.next_x + 20 >= self.display.width() {
                self.next_x = 20;
            }
            self.next_y += 20;
            if self.next_y + 20 >= self.display.height() {
                self.next_y = 20;
            }
        }

        self.order.push_front(id);
        self.windows.insert(id, Window::new(x, y, width, height, title));
        self.redraw = true;

        Ok(id)
    }

    fn read(&mut self, id: usize, buf: &mut [u8]) -> Result<usize> {
        if let Some(mut window) = self.windows.get_mut(&id) {
            window.read(buf)
        } else {
            Err(Error::new(EBADF))
        }
    }

    fn write(&mut self, id: usize, buf: &[u8]) -> Result<usize> {
        if let Some(mut window) = self.windows.get_mut(&id) {
            self.redraw = true;
            window.write(buf)
        } else {
            Err(Error::new(EBADF))
        }
    }

    fn close(&mut self, id: usize) -> Result<usize> {
        self.order.retain(|&e| e != id);

        if self.windows.remove(&id).is_some() {
            self.redraw = true;
            Ok(0)
        } else {
            Err(Error::new(EBADF))
        }
    }
}

fn main() {
    let mut socket = File::create(":orbital").unwrap();
    match Display::new() {
        Ok(display) => {
            println!("- Orbital: Found Display {}x{}", display.width(), display.height());
            println!("    Console: Press F1");
            println!("    Desktop: Press F2");

            Command::new("/apps/launcher/main.bin").spawn().unwrap();

            let mut scheme = OrbitalScheme::new(display);
            scheme.event_loop(&mut socket);
        },
        Err(err) => println!("- Orbital: No Display Found: {}", err)
    }
}