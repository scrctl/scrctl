#include "app/WindowAspect.h"
#include <SDL_syswm.h>
#import <AppKit/AppKit.h>

namespace scrctl::app {
void set_window_aspect(SDL_Window *window, int width, int height) {
    if (!window || width <= 0 || height <= 0) return;
    SDL_SysWMinfo info{};
    SDL_VERSION(&info.version);
    if (SDL_GetWindowWMInfo(window, &info) && info.subsystem == SDL_SYSWM_COCOA) {
        [info.info.cocoa.window setContentAspectRatio:NSMakeSize(width, height)];
    }
}
}
