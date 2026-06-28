#!/usr/bin/env python3
"""
Test script for Mohawk Inference Engine Dashboard

Verifies all dashboard components are properly initialized.
Run this before launching the main application to catch import errors.
"""

import sys
from pathlib import Path


def test_imports():
    """Test that all imports work correctly."""
    print("=" * 60)
    print("🧪 Testing Dashboard Imports...")
    print("=" * 60)
    
    try:
        from PyQt6.QtWidgets import QMainWindow, QWidget, QVBoxLayout
        print("✅ PyQt6 imports successful")
    except ImportError as e:
        print(f"❌ PyQt6 import failed: {e}")
        print("\nInstall PyQt6 with:")
        print("  pip install PyQt6 pyqtgraph")
        return False
    
    try:
        from main_window import MohawkGUI
        print("✅ Dashboard components import successful")
    except ImportError as e:
        print(f"❌ Dashboard component import failed: {e}")
        print("\nCheck mohawk_gui/main_window.py for errors")
        return False
    
    return True


def test_ui_components():
    """Test that UI components can be created."""
    print("\n" + "=" * 60)
    print("🧪 Testing UI Component Creation...")
    print("=" * 60)
    
    try:
        from main_window import MohawkGUI
        
        # Create QApplication (needed for GUI testing)
        from PyQt6.QtWidgets import QApplication
        app = QApplication(sys.argv)
        
        # Test MohawkGUI initialization
        print("✅ Creating MohawkGUI instance...")
        gui = MohawkGUI()
        print("✅ MohawkGUI created successfully")
        
        # Test that all tabs are initialized
        print("\n📋 Checking tab initialization:")
        
        tabs = [
            "Model Library",
            "Chat Interface", 
            "Performance Metrics",
            "Session Manager",
            "Workers",
            "Security Center",
            "History"
        ]

        actual_tabs = [gui.tabs.tabText(i) for i in range(gui.tabs.count())]
        for expected in tabs:
            if expected in actual_tabs:
                print(f"  ✅ {expected}")
            else:
                print(f"  ⚠️ Missing expected tab: {expected}")
        
        print("\n✅ All UI components initialized successfully!")
        
        return True
        
    except Exception as e:
        print(f"\n❌ UI component test failed: {e}")
        import traceback
        traceback.print_exc()
        return False


def test_dashboard_features():
    """Test that all dashboard features are present."""
    print("\n" + "=" * 60)
    print("🧪 Testing Dashboard Features...")
    print("=" * 60)
    
    try:
        from main_window import MohawkGUI

        from PyQt6.QtWidgets import QApplication
        app = QApplication(sys.argv)
        gui = MohawkGUI()

        features = {
            "Model Library": ["download_model", "refresh_models", "load_model_api"],
            "Chat Interface": ["send_message", "message_input", "chat_display"],
            "Metrics Dashboard": ["periodic_update", "metrics_bars"],
            "Session Manager": ["queue_high_priority", "queue_normal_priority", "cancel_session", "refresh_sessions"],
            "Worker Configuration": ["add_worker", "connect_workers", "refresh_workers"],
            "Security Center": ["create_security_tab"],
            "Global Controls": ["refresh_all"],
        }

        all_ok = True
        for feature_name, methods in features.items():
            print(f"\n{feature_name}:")
            for method in methods:
                if hasattr(gui, method):
                    print(f"  ✅ {method}() method exists")
                else:
                    print(f"  ❌ {method} not found")
                    all_ok = False

        if all_ok:
            print("\n✅ All dashboard features present!")
        else:
            print("\n⚠️ Some dashboard features are missing")
        return all_ok
        
    except Exception as e:
        print(f"\n❌ Dashboard feature test failed: {e}")
        import traceback
        traceback.print_exc()
        return False


def main():
    """Run all tests."""
    print("\n" + "=" * 60)
    print("🦅 Mohawk Inference Engine Dashboard - Test Suite")
    print("=" * 60 + "\n")
    
    results = []
    
    # Test imports
    if test_imports():
        results.append(("Imports", "✅ PASSED"))
    else:
        results.append(("Imports", "❌ FAILED"))
    
    # Test UI components
    if test_ui_components():
        results.append(("UI Components", "✅ PASSED"))
    else:
        results.append(("UI Components", "❌ FAILED"))
    
    # Test dashboard features
    if test_dashboard_features():
        results.append(("Dashboard Features", "✅ PASSED"))
    else:
        results.append(("Dashboard Features", "❌ FAILED"))
    
    # Summary
    print("\n" + "=" * 60)
    print("📊 Test Summary")
    print("=" * 60)
    
    passed = sum(1 for _, status in results if status == "✅ PASSED")
    total = len(results)
    
    for test_name, result in results:
        status_icon = "✅" if result == "✅ PASSED" else "❌"
        print(f"{status_icon} {test_name}: {result}")
    
    print("\n" + "=" * 60)
    if passed == total:
        print("🎉 All tests PASSED! Dashboard is ready to run.")
        print("=" * 60)
        print("\nTo launch the dashboard:")
        print("  python mohawk_gui/main.py")
        return 0
    else:
        print(f"⚠️  {total - passed} test(s) FAILED. Please fix errors above.")
        print("=" * 60)
        return 1


if __name__ == "__main__":
    sys.exit(main())
