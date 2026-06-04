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

package keyboard

import (
	"github.com/micmonay/keybd_event"
)

type KeySimulator interface {
	Copy() error
	Paste() error
}

type SystemKeySimulator struct{}

func NewSystemKeySimulator() KeySimulator {
	return &SystemKeySimulator{}
}

func (s *SystemKeySimulator) Copy() error {
	kb, err := keybd_event.NewKeyBonding()
	if err != nil {
		return err
	}
	kb.HasCTRL(true)
	kb.SetKeys(keybd_event.VK_C)
	return kb.Launching()
}

func (s *SystemKeySimulator) Paste() error {
	kb, err := keybd_event.NewKeyBonding()
	if err != nil {
		return err
	}
	kb.HasCTRL(true)
	kb.SetKeys(keybd_event.VK_V)
	return kb.Launching()
}
