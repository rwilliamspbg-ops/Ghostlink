#!/usr/bin/env python3
"""
Ghostlink GUI Comprehensive Test Runner

This script provides multiple testing modes and configurations:
1. Unit tests for individual components  
2. Integration tests for end-to-end workflows
3. Performance benchmarks 
4. GUI-specific functional tests
"""

import unittest
from pathlib import Path
import sys
import os
import json
import time
from typing import List, Dict, Any

# Add project root to path
sys.path.insert(0, str(Path(__file__).parent))

def setup_test_environment():
    """Set up testing environment."""
    print("Setting up test environment...")
    
    # Create directories for test artifacts if they don't exist
    test_dirs = [
        "test_results", 
        "test_logs",
        "test_artifacts"
    ]
    
    for directory in test_dirs:
        Path(directory).mkdir(exist_ok=True)

def run_unit_tests(test_suite, verbose: bool = True) -> dict:
    """Run unit tests with optional verbosity."""
    print("\n" + "="*60)
    print("RUNNING UNIT TESTS")
    print("="*60)
    
    if verbose:
        # More detailed output
        return unittest.TextTestRunner(verbosity=2).run(test_suite)
    else:
        # Minimal output  
        return unittest.TextTestRunner().run(test_suite)

def run_integration_tests(test_suite, verbose: bool = True) -> dict:
    """Run integration tests."""
    print("\n" + "="*60)
    print("RUNNING INTEGRATION TESTS")
    print("="*60)
    
    if verbose:
        return unittest.TextTestRunner(verbosity=2).run(test_suite)
    else:
        return unittest.TextTestRunner().run(test_suite)

def run_performance_benchmarks():
    """Run performance benchmarks."""
    from test_gui_framework import PerformanceTester
    
    print("\n" + "="*60)  
    print("RUNNING PERFORMANCE BENCHMARKS")
    print("="*60)
    
    try:
        tester = PerformanceTester()
        metrics = tester.profile_chat_performance()
        
        print(f"Performance Metrics: {metrics}")
        return True
    except Exception as e:
        print(f"Performance test failed: {e}")
        return False

def generate_test_report(results, test_type: str) -> dict:
    """Generate structured test report."""
    summary = {
        'type': f"{test_type}_summary",
        'timestamp': time.time(),
        'results': {
            'tests_run': results.testsRun if hasattr(results, 'testsRun') else 0,
            'failures_count': len(getattr(results, 'failures', [])),
            'errors_count': len(getattr(results, 'errors', [])),
            'success_rate': 1.0 - (len(getattr(results, 'failures', [])) + 
                              len(getattr(results, 'errors', []))) / max(1, results.testsRun if hasattr(results, 'testsRun') else 1),
        }
    }
    
    # Add any additional metadata
    summary['test_type'] = test_type
    
    return summary

def main():
    """Main function to run all GUI tests."""
    print("Ghostlink GUI Test Suite")
    print("=" * 60)
    print("Starting comprehensive GUI testing and validation...")
    
    try:
        # Setup environment first  
        setup_test_environment()
        
        # Run the different test suites
        import test_gui_framework
        
        # Create a suite with all main tests
        loader = unittest.TestLoader()
        basic_suite = unittest.TestSuite()

        for method_name in ['test_model_listing', 'test_model_loading',
                            'test_chat_with_system_prompt', 'test_health_endpoints']:
            try:
                basic_suite.addTest(test_gui_framework.TestGhostlinkGUI(methodName=method_name))
            except Exception as e:
                print(f"Warning: Could not add test {method_name}: {e}")
        
        # Run unit tests first
        print("1. Executing GUI component tests...")
        results = run_unit_tests(basic_suite, verbose=False)
        
        # Generate and display summary  
        report = generate_test_report(results, "component")
        print(f"\nTest Summary: {report['results']}")
        
        if not results.wasSuccessful():
            print("⚠️  Some GUI tests failed. Check output for details.")
            
        # Run performance benchmarks
        print("\n2. Executing performance testing...")
        perf_success = run_performance_benchmarks()
        
        # Final summary
        print("\n" + "="*60)
        print("TEST EXECUTION SUMMARY")
        print("="*60)
        if results.wasSuccessful() and perf_success:
            print("✅ All GUI tests passed successfully!")
            return 0
        else: 
            print("⚠️ Some tests had issues - see details above.")
            return 1
            
    except Exception as e:
        print(f"Critical error during test execution: {e}")
        import traceback
        traceback.print_exc()
        return 1

if __name__ == "__main__":
    exit_code = main()
    sys.exit(exit_code)