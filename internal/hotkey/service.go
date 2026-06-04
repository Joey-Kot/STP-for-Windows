// Copyright (C) 2026 Joey Kot <joey.kot.x@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed WITHOUT ANY WARRANTY; without even the
// implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See <https://www.gnu.org/licenses/> for more details.

package hotkey

type EventType int

const (
	TaskEvent EventType = iota + 1
	StopEvent
)

type Event struct {
	Type   EventType
	TaskID int
}

type Service interface {
	Start(handler func(Event)) error
	Close() error
}

type Options struct {
	UseHook        bool
	TaskHotkeys    map[int]string
	StopTaskHotkey string
	Debug          bool
}

func NewService(opts Options) Service {
	return newPlatformService(opts)
}
