#include <celero/Celero.h>
#include <humblelogging/api.h>

#include "dynamicbenchmark.h"

using namespace annis;

int main(int argc, char **argv) {
  
  humble::logging::Factory &fac = humble::logging::Factory::getInstance();
  fac.setConfiguration(humble::logging::DefaultConfiguration::createFromString(
          "logger.level(*)=info\n"
          ));
  fac.setDefaultFormatter(new humble::logging::PatternFormatter("[%date]- %m (%lls, %filename:%line)\n"));
  fac.registerAppender(new humble::logging::FileAppender("benchmark_annis4.log", true));
  
  if(argc > 1)
  {
    std::string benchmarkDir(argv[1]);
    
    // find all sub-directories of the "queries" folder
    boost::filesystem::directory_iterator fileEndIt;
    boost::filesystem::directory_iterator itFiles(benchmarkDir + "/queries");
    while(itFiles != fileEndIt)
    {
      if(itFiles->status().type() == boost::filesystem::directory_file)
      {
        const auto subdirPath = itFiles->path();
        std::string subdir = subdirPath.string();
        std::string corpusName = subdirPath.filename().string();
        
        // get the corpus path (is subfolder of "data" folder)
        std::string corpusPath = benchmarkDir + "/data/" + corpusName;
        
        DynamicBenchmark benchmark(subdir, corpusPath, corpusName
          , true);

        unsigned int numOfCPUs = std::thread::hardware_concurrency();
        std::shared_ptr<ThreadPool> sharedThreadPool = std::make_shared<ThreadPool>(numOfCPUs);

        for(int i=0; i <= numOfCPUs; i += 2)
        {
          QueryConfig config;
          config.threadPool = i > 0 ? sharedThreadPool : nullptr;
          config.numOfBackgroundTasks = i;
          benchmark.registerFixture("Jobs_" + std::to_string(i), config);
        }

      }
      itFiles++;
    }
    
    
    celero::Run(argc, argv);
    
  }
  else
  {
    std::cout << "You have to give a benchmark directy (which contains a \"queries\" and \"data\" sub-directory) as argument."
      << std::endl;
  }

  return 0;
}
