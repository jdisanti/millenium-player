// This file is part of Millenium Player.
// Copyright (C) 2023 John DiSanti.
//
// Millenium Player is free software: you can redistribute it and/or modify it under the terms of
// the GNU General Public License as published by the Free Software Foundation, either version 3 of
// the License, or (at your option) any later version.
//
// Millenium Player is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See
// the GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along with Millenium Player.
// If not, see <https://www.gnu.org/licenses/>.

use crate::{error, warn};
use gloo::utils::window;
use js_sys::Float32Array;
use millenium_post_office::frontend::state::WaveformStateData;
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{prelude::Closure, JsCast};
use web_sys::{
    HtmlCanvasElement, WebGlBuffer, WebGlProgram, WebGlRenderingContext as GL, WebGlUniformLocation,
};
use yew::prelude::*;

const WIDTH: f32 = 400.0;
const HEIGHT: f32 = 200.0;

#[derive(Properties, PartialEq)]
pub struct WaveformProps {
    pub waveform: Rc<RefCell<WaveformStateData>>,
}

pub struct Waveform {
    canvas_ref: NodeRef,
}

impl Component for Waveform {
    type Message = ();
    type Properties = WaveformProps;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            canvas_ref: NodeRef::default(),
        }
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
        html! {
            <canvas class="waveform" ref={self.canvas_ref.clone()}></canvas>
        }
    }

    fn rendered(&mut self, ctx: &Context<Self>, first_render: bool) {
        if first_render {
            let canvas = self
                .canvas_ref
                .cast::<HtmlCanvasElement>()
                .expect("failed to get canvas");
            let gl: GL = match canvas.get_context("webgl") {
                Ok(Some(context)) => context
                    .dyn_into()
                    .expect("failed to cast JsObject into WebGlRenderContext"),
                Ok(None) => {
                    warn!("webview doesn't support WebGL");
                    return;
                }
                Err(err) => {
                    error!("failed to call HtmlCanvasElement::getContext: {err:?}");
                    return;
                }
            };
            Self::setup_render_loop(gl, ctx.props().waveform.clone());
        }
    }
}

impl Waveform {
    fn request_animation_frame(render: &Closure<dyn FnMut()>) {
        window()
            .request_animation_frame(render.as_ref().unchecked_ref())
            .expect("failed to request animation frame");
    }

    fn setup_render_loop(gl: GL, waveform: Rc<RefCell<WaveformStateData>>) {
        let waveform_bin_count = waveform.borrow().waveform.as_ref().unwrap().spectrum.len() as f32;
        let resources = match create_gl_resources(&gl, waveform_bin_count) {
            Ok(resources) => resources,
            Err(err) => {
                error!("{err}");
                return;
            }
        };

        let animation_frame_callback = Rc::new(RefCell::new(None));
        *animation_frame_callback.borrow_mut() = Some(Closure::wrap(Box::new({
            let animation_frame_callback = animation_frame_callback.clone();
            move || {
                Self::render(gl.clone(), resources.clone(), waveform.clone());
                Waveform::request_animation_frame(
                    animation_frame_callback.borrow().as_ref().unwrap(),
                );
            }
        })
            as Box<dyn FnMut()>));

        Waveform::request_animation_frame(animation_frame_callback.borrow().as_ref().unwrap());
    }

    fn render(gl: GL, resources: Rc<Resources>, waveform: Rc<RefCell<WaveformStateData>>) {
        gl.clear_color(0.0, 0.0, 0.0, 1.0);
        gl.clear(GL::COLOR_BUFFER_BIT);

        let waveform = waveform.borrow();
        let waveform = waveform.waveform.as_ref().unwrap();
        let bin_count = waveform.spectrum.len() as f32;

        let center_y = (0.33 * HEIGHT).round();
        let top_scale = 0.8;
        let bottom_scale = 0.4;
        let step = (WIDTH / bin_count).round();

        for (i, &height) in waveform.spectrum.iter().enumerate() {
            gl.uniform1f(Some(&resources.uniform_offset_x), step * i as f32);
            gl.uniform1f(Some(&resources.uniform_offset_y), center_y);
            gl.uniform1f(Some(&resources.uniform_scale_y), height * top_scale);
            gl.draw_arrays(GL::TRIANGLES, 0, 4 * 6);
        }
        for (i, &height) in waveform.amplitude.iter().enumerate() {
            gl.uniform1f(Some(&resources.uniform_offset_x), step * i as f32);
            gl.uniform1f(Some(&resources.uniform_offset_y), center_y);
            gl.uniform1f(Some(&resources.uniform_scale_y), -height * bottom_scale);
            gl.draw_arrays(GL::TRIANGLES, 0, 4 * 6);
        }
    }
}

struct Resources {
    _shader_program: WebGlProgram,
    _position_buffer: WebGlBuffer,
    _color_buffer: WebGlBuffer,
    uniform_scale_y: WebGlUniformLocation,
    uniform_offset_y: WebGlUniformLocation,
    uniform_offset_x: WebGlUniformLocation,
    _uniform_view_matrix: WebGlUniformLocation,
}

fn compile_shader(gl: &GL, vertex_code: &str, fragment_code: &str) -> Result<WebGlProgram, String> {
    let vertex_shader = gl
        .create_shader(GL::VERTEX_SHADER)
        .expect("failed to create vertex shader");
    gl.shader_source(&vertex_shader, vertex_code);
    gl.compile_shader(&vertex_shader);

    let fragment_shader = gl
        .create_shader(GL::FRAGMENT_SHADER)
        .expect("failed to create fragment shader");
    gl.shader_source(&fragment_shader, fragment_code);
    gl.compile_shader(&fragment_shader);

    let shader_program = gl
        .create_program()
        .expect("failed to create shader program");
    gl.attach_shader(&shader_program, &vertex_shader);
    gl.attach_shader(&shader_program, &fragment_shader);
    gl.link_program(&shader_program);
    if !gl.get_program_parameter(&shader_program, GL::LINK_STATUS) {
        let message = gl
            .get_program_info_log(&shader_program)
            .unwrap_or_else(|| "no error".into());
        return Err(format!("failed to link the shader program: {message}"));
    }
    Ok(shader_program)
}

fn create_buffer_f32(gl: &GL, values: &[f32]) -> WebGlBuffer {
    let buffer = gl.create_buffer().expect("failed to create buffer");
    gl.bind_buffer(GL::ARRAY_BUFFER, Some(&buffer));
    gl.buffer_data_with_array_buffer_view(
        GL::ARRAY_BUFFER,
        &Float32Array::from(values),
        GL::STATIC_DRAW,
    );
    buffer
}

fn bind_f32_array_buffer_attr(
    gl: &GL,
    element_size_bytes: i32,
    shader: &WebGlProgram,
    buffer: &WebGlBuffer,
    attribute_name: &str,
) {
    gl.bind_buffer(GL::ARRAY_BUFFER, Some(buffer));
    let location = gl.get_attrib_location(shader, attribute_name);
    assert!(
        location >= 0,
        "failed to find `{attribute_name}` in the vertex shader"
    );
    gl.vertex_attrib_pointer_with_i32(location as u32, element_size_bytes, GL::FLOAT, false, 0, 0);
    gl.enable_vertex_attrib_array(location as u32);
}

fn create_buffers(gl: &GL, waveform_bin_count: f32) -> (WebGlBuffer, WebGlBuffer) {
    let w = (WIDTH / waveform_bin_count - 1.0).floor();
    let h = (HEIGHT / 4.0).round();
    let position_buffer = {
        let mut positions: Vec<f32> = Vec::new();
        for f in 0..4 {
            let (left, right) = (0.0, w);
            let (bottom, top) = (f as f32 * h, (f + 1) as f32 * h);
            positions.extend(&[left, bottom]);
            positions.extend(&[left, top]);
            positions.extend(&[right, bottom]);
            positions.extend(&[left, top]);
            positions.extend(&[right, top]);
            positions.extend(&[right, bottom]);
        }
        create_buffer_f32(gl, &positions)
    };
    let color_buffer = {
        let colors = &[
            [0.25, 0.0, 0.0, 1.0],
            [0.5, 0.0, 0.0, 1.0],
            [0.75, 0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 1.0],
        ];
        let mut buffer: Vec<f32> = Vec::new();
        for color in colors {
            for _ in 0..6 {
                buffer.extend(color);
            }
        }
        create_buffer_f32(gl, &buffer)
    };
    (position_buffer, color_buffer)
}

fn create_gl_resources(gl: &GL, waveform_bin_count: f32) -> Result<Rc<Resources>, String> {
    let vertex_code = r#"
            precision mediump float;
            attribute vec2 attr_position;
            attribute vec4 attr_color;
            uniform float offset_x;
            uniform float offset_y;
            uniform float scale_y;
            uniform mat4 view_matrix;
            varying vec4 varying_color;

            void main() {
                gl_Position = view_matrix * vec4(
                    attr_position.x + offset_x,
                    attr_position.y * scale_y + offset_y,
                    0.0,
                    1.0
                );
                varying_color = attr_color;
            }
        "#;
    let fragment_code = r#"
            precision mediump float;
            varying vec4 varying_color;

            void main() {
                gl_FragColor = varying_color;
            }
        "#;
    let shader_program = compile_shader(gl, vertex_code, fragment_code)?;
    gl.use_program(Some(&shader_program));

    let (position_buffer, color_buffer) = create_buffers(gl, waveform_bin_count);
    bind_f32_array_buffer_attr(gl, 2, &shader_program, &position_buffer, "attr_position");
    bind_f32_array_buffer_attr(gl, 4, &shader_program, &color_buffer, "attr_color");

    let uniform_offset_x = gl
        .get_uniform_location(&shader_program, "offset_x")
        .expect("failed to find `offset_x` uniform");
    gl.uniform1f(Some(&uniform_offset_x), 0.0);

    let uniform_offset_y = gl
        .get_uniform_location(&shader_program, "offset_y")
        .expect("failed to find `offset_y` uniform");
    gl.uniform1f(Some(&uniform_offset_y), 0.0);

    let uniform_scale_y = gl
        .get_uniform_location(&shader_program, "scale_y")
        .expect("failed to find `scale_y` uniform");
    gl.uniform1f(Some(&uniform_scale_y), 1.0);

    let uniform_view_matrix = gl
        .get_uniform_location(&shader_program, "view_matrix")
        .expect("failed to find `view_matrix` uniform");

    // Transform x=[0..400], y=[0..200] to x=[0..2], y=[0..2]
    // Transform x=2x-1, y=2y-1 to get to x=[-1..1], y=[-1..1]
    #[rustfmt::skip]
    gl.uniform_matrix4fv_with_f32_array(Some(&uniform_view_matrix), false, &[
        2.0 / WIDTH, 0.0,          0.0,  0.0,
        0.0,         2.0 / HEIGHT, 0.0,  0.0,
        0.0,         0.0,          1.0,  0.0,
       -1.0,        -1.0,          0.0,  1.0,
    ]);

    Ok(Rc::new(Resources {
        _shader_program: shader_program,
        _position_buffer: position_buffer,
        _color_buffer: color_buffer,
        uniform_offset_x,
        uniform_offset_y,
        uniform_scale_y,
        _uniform_view_matrix: uniform_view_matrix,
    }))
}
