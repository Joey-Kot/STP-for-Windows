//go:build windows

package hotkey

import (
	"fmt"
	"runtime"
	"strings"
	"sync"
	"syscall"
	"time"
	"unsafe"
)

const (
	whKeyboardLL  = 13
	llkhfInjected = 0x00000010
	wmKeyDown     = 0x0100
	wmKeyUp       = 0x0101
	wmSysKeyDown  = 0x0104
	wmSysKeyUp    = 0x0105
	wmHotkey      = 0x0312

	vkControl = 0x11
	vkMenu    = 0x12
	vkShift   = 0x10
	vkLwin    = 0x5B
	vkRwin    = 0x5C

	stopHotkeyID = 1000001
)

type taskSpec struct {
	id  int
	mod uint32
	vk  uint32
}

type platformService struct {
	opts Options

	events  chan Event
	handler func(Event)
	stopCh  chan struct{}

	// register hotkey mode
	registered []int

	// low-level hook mode
	llHookHandle uintptr
	llHotkeyMap  map[uint32][]taskSpec
	llBlocked    map[uint32]bool
	llBlockedMu  sync.Mutex

	procCallNextHookEx   *syscall.LazyProc
	procGetAsyncKeyState *syscall.LazyProc
}

var activeLLService *platformService

func newPlatformService(opts Options) Service {
	return &platformService{
		opts:      opts,
		events:    make(chan Event, 32),
		stopCh:    make(chan struct{}),
		llBlocked: make(map[uint32]bool),
	}
}

func (s *platformService) Start(handler func(Event)) error {
	s.handler = handler
	go s.runDispatcher()
	if s.opts.UseHook {
		return s.startLowLevelHook()
	}
	return s.startRegisterHotkey()
}

func (s *platformService) Close() error {
	close(s.stopCh)
	return nil
}

func (s *platformService) runDispatcher() {
	for {
		select {
		case <-s.stopCh:
			return
		case ev := <-s.events:
			s.handler(ev)
		}
	}
}

func (s *platformService) buildEntries() ([]taskSpec, error) {
	entries := make([]taskSpec, 0)
	for id, spec := range s.opts.TaskHotkeys {
		if strings.TrimSpace(spec) == "" {
			continue
		}
		mod, vk, err := ParseHotkey(spec)
		if err != nil {
			return nil, fmt.Errorf("invalid hotkey '%s' for id=%d: %v", spec, id, err)
		}
		entries = append(entries, taskSpec{id: id, mod: mod, vk: vk})
	}
	if strings.TrimSpace(s.opts.StopTaskHotkey) != "" {
		mod, vk, err := ParseHotkey(s.opts.StopTaskHotkey)
		if err != nil {
			return nil, fmt.Errorf("invalid stop hotkey '%s': %v", s.opts.StopTaskHotkey, err)
		}
		entries = append(entries, taskSpec{id: stopHotkeyID, mod: mod, vk: vk})
	}
	return entries, nil
}

func (s *platformService) emitByID(id int) {
	ev := Event{Type: TaskEvent, TaskID: id}
	if id == stopHotkeyID {
		ev = Event{Type: StopEvent}
	}
	select {
	case s.events <- ev:
	default:
	}
}

func (s *platformService) startRegisterHotkey() error {
	entries, err := s.buildEntries()
	if err != nil {
		return err
	}
	if len(entries) == 0 {
		return nil
	}
	errCh := make(chan error, 1)
	go func() {
		runtime.LockOSThread()
		defer runtime.UnlockOSThread()

		user32 := syscall.NewLazyDLL("user32.dll")
		reg := user32.NewProc("RegisterHotKey")
		unreg := user32.NewProc("UnregisterHotKey")
		getMsg := user32.NewProc("GetMessageW")

		for _, e := range entries {
			r, _, _ := reg.Call(0, uintptr(e.id), uintptr(e.mod), uintptr(e.vk))
			if r == 0 {
				for _, id := range s.registered {
					unreg.Call(0, uintptr(id))
				}
				errCh <- fmt.Errorf("RegisterHotKey failed for id=%d", e.id)
				return
			}
			s.registered = append(s.registered, e.id)
		}
		errCh <- nil

		var msg struct {
			Hwnd    uintptr
			Message uint32
			WParam  uintptr
			LParam  uintptr
			Time    uint32
			PtX     int32
			PtY     int32
		}
		for {
			ret, _, _ := getMsg.Call(uintptr(unsafe.Pointer(&msg)), 0, 0, 0)
			if int32(ret) == -1 {
				return
			}
			if msg.Message == wmHotkey {
				s.emitByID(int(msg.WParam))
			}
		}
	}()

	select {
	case err := <-errCh:
		return err
	case <-time.After(2 * time.Second):
		return fmt.Errorf("timeout registering hotkeys")
	}
}

func (s *platformService) startLowLevelHook() error {
	entries, err := s.buildEntries()
	if err != nil {
		return err
	}
	if len(entries) == 0 {
		return nil
	}
	s.llHotkeyMap = make(map[uint32][]taskSpec)
	for _, e := range entries {
		s.llHotkeyMap[e.vk] = append(s.llHotkeyMap[e.vk], e)
	}

	errCh := make(chan error, 1)
	go func() {
		runtime.LockOSThread()
		defer runtime.UnlockOSThread()

		user32 := syscall.NewLazyDLL("user32.dll")
		setHook := user32.NewProc("SetWindowsHookExW")
		unhook := user32.NewProc("UnhookWindowsHookEx")
		getMsg := user32.NewProc("GetMessageW")
		s.procCallNextHookEx = user32.NewProc("CallNextHookEx")
		s.procGetAsyncKeyState = user32.NewProc("GetAsyncKeyState")

		activeLLService = s
		cb := syscall.NewCallback(lowLevelKeyboardProc)
		h, _, e := setHook.Call(uintptr(whKeyboardLL), cb, 0, 0)
		if h == 0 {
			errCh <- fmt.Errorf("SetWindowsHookExW failed: %v", e)
			return
		}
		s.llHookHandle = h
		errCh <- nil

		var msg struct {
			Hwnd    uintptr
			Message uint32
			WParam  uintptr
			LParam  uintptr
			Time    uint32
			PtX     int32
			PtY     int32
		}
		for {
			ret, _, _ := getMsg.Call(uintptr(unsafe.Pointer(&msg)), 0, 0, 0)
			if int32(ret) == -1 {
				break
			}
		}
		unhook.Call(s.llHookHandle)
	}()

	select {
	case err := <-errCh:
		return err
	case <-time.After(2 * time.Second):
		return fmt.Errorf("timeout installing low-level keyboard hook")
	}
}

func lowLevelKeyboardProc(nCode int, wParam uintptr, lParam uintptr) uintptr {
	s := activeLLService
	if s == nil || nCode < 0 {
		if s != nil && s.procCallNextHookEx != nil {
			ret, _, _ := s.procCallNextHookEx.Call(0, uintptr(nCode), wParam, lParam)
			return ret
		}
		return 0
	}
	k := (*struct {
		VkCode      uint32
		ScanCode    uint32
		Flags       uint32
		Time        uint32
		DwExtraInfo uintptr
	})(unsafe.Pointer(lParam))

	if (k.Flags & llkhfInjected) != 0 {
		ret, _, _ := s.procCallNextHookEx.Call(0, uintptr(nCode), wParam, lParam)
		return ret
	}

	msg := uint32(wParam)
	switch msg {
	case wmKeyDown, wmSysKeyDown:
		entries := s.llHotkeyMap[k.VkCode]
		for _, e := range entries {
			if s.modsMatch(e.mod) {
				s.llBlockedMu.Lock()
				s.llBlocked[k.VkCode] = true
				s.llBlockedMu.Unlock()
				s.emitByID(e.id)
				return 1
			}
		}
	case wmKeyUp, wmSysKeyUp:
		s.llBlockedMu.Lock()
		blocked := s.llBlocked[k.VkCode]
		if blocked {
			delete(s.llBlocked, k.VkCode)
			s.llBlockedMu.Unlock()
			return 1
		}
		s.llBlockedMu.Unlock()
	}

	ret, _, _ := s.procCallNextHookEx.Call(0, uintptr(nCode), wParam, lParam)
	return ret
}

func (s *platformService) modsMatch(required uint32) bool {
	isDown := func(vk int) bool {
		if s.procGetAsyncKeyState == nil {
			return false
		}
		r, _, _ := s.procGetAsyncKeyState.Call(uintptr(vk))
		return int32(r)&0x8000 != 0
	}
	if required&0x0001 != 0 && !isDown(vkMenu) {
		return false
	}
	if required&0x0002 != 0 && !isDown(vkControl) {
		return false
	}
	if required&0x0004 != 0 && !isDown(vkShift) {
		return false
	}
	if required&0x0008 != 0 && !(isDown(vkLwin) || isDown(vkRwin)) {
		return false
	}
	return true
}
