#pragma once
#include <SDL.h>

namespace scrctl::app {
// SDL2 has no portable aspect-ratio constraint. Cocoa supports it natively.
#ifdef __APPLE__
void set_window_aspect(SDL_Window *window, int width, int height);
#else
inline void set_window_aspect(SDL_Window *, int, int) {}
#endif
}
