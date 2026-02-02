package clipboard

import (
	"fmt"
	"strings"
	"time"

	sysclipboard "github.com/atotto/clipboard"

	"stp/internal/keyboard"
)

type Clipboard interface {
	ReadAll() (string, error)
	WriteAll(text string) error
}

type SystemClipboard struct{}

func (s *SystemClipboard) ReadAll() (string, error) {
	return sysclipboard.ReadAll()
}

func (s *SystemClipboard) WriteAll(text string) error {
	return sysclipboard.WriteAll(text)
}

type TextIO interface {
	CopySelected() (string, error)
	PasteText(text string) error
}

type Manager struct {
	Clipboard Clipboard
	Keyboard  keyboard.KeySimulator
	Timeout   time.Duration
	Debug     bool
}

func (m *Manager) CopySelected() (string, error) {
	orig, _ := m.Clipboard.ReadAll()
	defer func() {
		time.Sleep(150 * time.Millisecond)
		for i := 0; i < 5; i++ {
			if err := m.Clipboard.WriteAll(orig); err == nil {
				break
			}
			time.Sleep(50 * time.Millisecond)
		}
	}()

	for i := 0; i < 5; i++ {
		if err := m.Clipboard.WriteAll(""); err == nil {
			break
		}
		time.Sleep(50 * time.Millisecond)
	}
	time.Sleep(50 * time.Millisecond)
	if err := m.Keyboard.Copy(); err != nil {
		return "", err
	}
	timeout := time.After(m.Timeout)
	ticker := time.NewTicker(50 * time.Millisecond)
	defer ticker.Stop()
	for {
		select {
		case <-timeout:
			return "", fmt.Errorf("timeout waiting for clipboard after Ctrl+C")
		case <-ticker.C:
			res, err := m.Clipboard.ReadAll()
			if err != nil {
				continue
			}
			if strings.TrimSpace(res) != "" {
				return res, nil
			}
		}
	}
}

func (m *Manager) PasteText(text string) error {
	orig, _ := m.Clipboard.ReadAll()
	defer func() {
		time.Sleep(120 * time.Millisecond)
		for i := 0; i < 5; i++ {
			if err := m.Clipboard.WriteAll(orig); err == nil {
				break
			}
			time.Sleep(50 * time.Millisecond)
		}
	}()

	for i := 0; i < 5; i++ {
		if err := m.Clipboard.WriteAll(text); err == nil {
			time.Sleep(80 * time.Millisecond)
			return m.Keyboard.Paste()
		}
		time.Sleep(50 * time.Millisecond)
	}
	return fmt.Errorf("failed to write clipboard")
}
