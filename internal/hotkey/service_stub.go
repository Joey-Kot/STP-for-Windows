//go:build !windows

package hotkey

import "fmt"

type unsupportedService struct{}

func newPlatformService(opts Options) Service {
	return &unsupportedService{}
}

func (s *unsupportedService) Start(handler func(Event)) error {
	return fmt.Errorf("hotkey service is only supported on Windows")
}

func (s *unsupportedService) Close() error {
	return nil
}
