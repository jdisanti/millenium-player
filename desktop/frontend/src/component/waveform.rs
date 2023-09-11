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

use crate::error;
use gloo::utils::window;
use js_sys::Float32Array;
use millenium_post_office::frontend::state::WaveformStateData;
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{prelude::Closure, JsCast};
use web_sys::{
    HtmlCanvasElement, WebGlBuffer, WebGlProgram, WebGlRenderingContext as GL, WebGlUniformLocation,
};
use yew::prelude::*;

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
            let gl: GL = canvas
                .get_context("webgl")
                .expect("failed to get webgl context")
                .expect("no webgl context available")
                .dyn_into()
                .expect("failed to cast JsObject into WebGlRenderingContext");
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

        let center_y = 0.33;
        let top_height = 0.66 * 1.2;
        let bottom_height = 0.33 * 1.2;

        for (i, &height) in waveform.spectrum.iter().enumerate() {
            gl.uniform1f(
                Some(&resources.uniform_horizontal_offset),
                i as f32 / bin_count,
            );
            gl.uniform1f(Some(&resources.uniform_vertical_offset), center_y);
            gl.uniform1f(Some(&resources.uniform_vertical_scale), height * top_height);
            gl.draw_arrays(GL::TRIANGLES, 0, 4 * 6);
        }
        for (i, &height) in waveform.amplitude.iter().enumerate() {
            gl.uniform1f(
                Some(&resources.uniform_horizontal_offset),
                i as f32 / bin_count,
            );
            gl.uniform1f(Some(&resources.uniform_vertical_offset), center_y);
            gl.uniform1f(
                Some(&resources.uniform_vertical_scale),
                -height * bottom_height,
            );
            gl.draw_arrays(GL::TRIANGLES, 0, 4 * 6);
        }
    }
}

struct Resources {
    _shader_program: WebGlProgram,
    _position_buffer: WebGlBuffer,
    _color_buffer: WebGlBuffer,
    uniform_vertical_scale: WebGlUniformLocation,
    uniform_vertical_offset: WebGlUniformLocation,
    uniform_horizontal_offset: WebGlUniformLocation,
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
    let w = 1.0 / waveform_bin_count - 1.0 / 400.0;
    let h = 1.0 / 4.0;
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
            [0.25, 0.0, 0.0, 0.0],
            [0.5, 0.0, 0.0, 0.0],
            [0.75, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
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
            uniform float horizontal_offset;
            uniform float vertical_offset;
            uniform float vertical_scale;
            varying vec4 varying_color;

            void main() {
                gl_Position = vec4(
                    (horizontal_offset + attr_position.x) * 2.0 - 1.0,
                    (vertical_offset + attr_position.y * vertical_scale) * 2.0 - 1.0,
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

    let uniform_horizontal_offset = gl
        .get_uniform_location(&shader_program, "horizontal_offset")
        .expect("failed to find `horizontal_offset` uniform");
    gl.uniform1f(Some(&uniform_horizontal_offset), 0.0);

    let uniform_vertical_offset = gl
        .get_uniform_location(&shader_program, "vertical_offset")
        .expect("failed to find `vertical_offset` uniform");
    gl.uniform1f(Some(&uniform_vertical_offset), 0.0);

    let uniform_vertical_scale = gl
        .get_uniform_location(&shader_program, "vertical_scale")
        .expect("failed to find `vertical_scale` uniform");
    gl.uniform1f(Some(&uniform_vertical_scale), 1.0);

    Ok(Rc::new(Resources {
        _shader_program: shader_program,
        _position_buffer: position_buffer,
        _color_buffer: color_buffer,
        uniform_horizontal_offset,
        uniform_vertical_offset,
        uniform_vertical_scale,
    }))
}
