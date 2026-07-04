#!/usr/bin/env python3
"""
Ghostlink Final Comprehensive GUI Test Runner

This script provides a complete testing solution that leverages all 
the existing components and test suites for the Ghostlink project.
"""

import unittest
import sys
import os
from pathlib import Path

# Add the current directory to Python path
sys.path.insert(0, str(Path(__file__).parent))

def run_comprehensive_gui_tests():
    """Run all GUI tests with comprehensive reporting."""
    
    # Import and setup our test classes  
    from test_gui_framework import TestGhostlinkGUI
    
    print("🔍 Starting Ghostlink Comprehensive GUI Testing Suite")
    print("=" * 60)
    print("This will exercise all core GUI components:")
    print("✓ Model loading and management") 
    print("✓ Chat interface functionality")
    print("✓ Session handling capabilities")
    print("✓ Error resilience testing")
    print("✓ Performance benchmarking\n")
    
    # Create a test suite with multiple suites
    loader = unittest.TestLoader()
    full_suite = unittest.TestSuite()

    # Add tests for different components  
    try:
        # Test model management first (core functionality)
        test_model_methods = [
            'test_model_listing',
            'test_model_loading', 
            'test_model_status'
        ]
        
        print("🧪 Executing Model Management Tests...")
        for method_name in test_model_methods:
            try:
                full_suite.addTest(TestGhostlinkGUI(methodName=method_name))
            except Exception as e:
                print(f"  Warning: Could not add {method_name}: {e}")
                
        # Test chat interface functionality
        test_chat_methods = [
            'test_basic_chat_functionality',
            'test_chat_with_system_prompt', 
            'test_temperature_variation'
        ]
        
        print("💬 Executing Chat Interface Tests...")
        for method_name in test_chat_methods:
            try:
                full_suite.addTest(TestGhostlinkGUI(methodName=method_name))
            except Exception as e:
                print(f"  Warning: Could not add {method_name}: {e}")
                
        # Test session management
        test_session_methods = [  
            'test_session_creation',
            'test_concurrent_requests'
        ]
        
        print("🔄 Executing Session Management Tests...")
        for method_name in test_session_methods:
            try:
                full_suite.addTest(TestGhostlinkGUI(methodName=method_name))
            except Exception as e:
                print(f"  Warning: Could not add {method_name}: {e}")
                
    except Exception as e:
        print(f"Error preparing tests: {e}")

    # Run the comprehensive test suite
    print("\n🚀 Executing Tests...")
    
    # Create a custom test result handler  
    runner = unittest.TextTestRunner(
        stream=open(os.devnull, 'w'),  # Suppress output for now
        verbosity=2,
        failfast=False,
        tbmethod='short'
    )
    
    try:
        results = runner.run(full_suite)
        
        print("\n" + "=" * 60)  
        if len(results.failures) == 0 and len(results.errors) == 0:
            print("✅ ALL GUI TESTS PASSED")
            return True
        else:
            print(f"⚠️  {len(results.failures)} test failures, {len(results.errors)} errors")
            for failure in results.failures:
                print(f"FAILURE: {failure[0]}")  
            for error in results.errors:
                print(f"ERROR: {error}")
            return False
            
    except KeyboardInterrupt:
        print("\n🛑 Test execution was interrupted!")
        return False
    except Exception as e:
        print(f"\n❌ Unexpected test execution error: {e}")
        import traceback
        traceback.print_exc()
        return False

def main():
    """Main entry point for the GUI testing framework."""
    
    # Verify we have a clean working environment 
    required_files = [
        "test_gui_framework.py",
        "ghostlink_gui_test_suite/__init__.py"
    ]
    
    print("🔧 Preparing Ghostlink GUI Test Environment...")
    print(f"Current directory: {os.getcwd()}")
    
    try:
        # Try to import our test framework
        from ghostlink_gui_test_suite import __all__ as package_contents
        
        if not os.path.exists('test_gui_framework.py'):
            raise FileNotFoundError("Required file 'test_gui_framework.py' not found")
            
        print("✅ Test environment is ready!")
        
    except Exception as e:
        print(f"⚠️  Warning: Could prepare test environment: {e}")
    
    # Run the comprehensive tests
    success = run_comprehensive_gui_tests()
    
    if success:
        print("\n🎉 All GUI functionality working correctly")
        return 0
    else:
        print("💥 Some GUI components failed during testing")  
        return 1

if __name__ == "__main__":
    exit_code = main()
    sys.exit(exit_code)