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
