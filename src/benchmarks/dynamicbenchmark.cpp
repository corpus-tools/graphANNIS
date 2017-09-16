/*
   Copyright 2017 Thomas Krause <thomaskrause@posteo.de>

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
*/

#include "dynamicbenchmark.h"
#include <annis/json/jsonqueryparser.h>

#include <humblelogging/api.h>
#include <boost/filesystem.hpp>
#include <boost/filesystem/fstream.hpp>
#include <string>
#include <stddef.h>

#include <cmath>

using namespace annis;

HUMBLE_LOGGER(benchLogger, "DynamicBenchmark");

std::shared_ptr<DBCache> DynamicCorpusFixture::dbCache
  = std::make_shared<DBCache>(0);

void DynamicCorpusFixture::UserBenchmark()
{

  if(timeout > 0)
  {
    runner = boost::thread([this] {
      while (this->q->next())
      {
        this->counter++;
        boost::this_thread::interruption_point();
      }
    });

    if(runner.try_join_for(boost::chrono::milliseconds(timeout)))
    {
      HL_INFO(benchLogger, (boost::format("result %1%") % counter).str());
      if (expectedCount && counter != *expectedCount)
      {
        std::cerr << "FATAL ERROR: query " << benchmarkName << ":" << currentExperimentValue << " should have count " << *expectedCount << " but was " << counter << std::endl;
        std::cerr << "" << __FILE__ << ":" << __LINE__ << std::endl;
        exit(-1);
      }
    }
  }
  else
  {
    while (this->q->next())
    {
      this->counter++;
      boost::this_thread::interruption_point();
    }

    HL_INFO(benchLogger, (boost::format("result %1%") % counter).str());
    if (expectedCount && counter != *expectedCount)
    {
      std::cerr << "FATAL ERROR: query " << benchmarkName << ":" << currentExperimentValue << " should have count " << *expectedCount << " but was " << counter << std::endl;
      std::cerr << "" << __FILE__ << ":" << __LINE__ << std::endl;
      exit(-1);
    }
  }
}

std::vector<std::pair<int64_t, uint64_t> > DynamicCorpusFixture::getExperimentValues() const
{
  std::vector<std::pair<int64_t, uint64_t> > result;

  for (auto it : json)
  {
    result.push_back({it.first, 0});
  }

  return result;
}

void DynamicCorpusFixture::tearDown()
{
  if(runner.joinable())
  {
    runner.interrupt();
    runner.join();
    HL_INFO(benchLogger, (boost::format("timeout")).str());
  }
}

DynamicBenchmark::DynamicBenchmark(std::string queriesDir,
  std::string corpusPath, std::string benchmarkName, int64_t timeout, bool multipleExperimentsParam)
  : corpusPath(corpusPath), benchmarkName(benchmarkName), multipleExperiments(multipleExperimentsParam),
    timeout(timeout)
{
  // find all file ending with ".json" in the folder
  boost::filesystem::directory_iterator fileEndIt;

  boost::filesystem::directory_iterator itFiles(queriesDir);
  while (itFiles != fileEndIt)
  {
    const auto filePath = itFiles->path();
    if (filePath.extension().string() == ".json")
    {
      if(multipleExperiments)
      {
        // check if the file name is a valid number
        std::string name = filePath.filename().stem().string();
        try
        {
          std::stol(name);
        }
        catch(std::invalid_argument invalid)
        {
          // not a number, don't assume we have multiple experiments
          multipleExperiments = false;
        }
      }
       
      foundJSONFiles.push_back(filePath);
    }
    itFiles++;
  }
  
  if(foundJSONFiles.empty())
  {
    multipleExperiments = false;
  }

  QueryConfig baselineConfig;
  registerFixtureInternal(true, "baseline", timeout, baselineConfig);
}

void DynamicBenchmark::registerFixture(std::string fixtureName, const QueryConfig config)
{
  registerFixtureInternal(false, fixtureName, timeout, config);
}

void DynamicBenchmark::registerFixtureInternal(
  bool baseline,
  std::string fixtureName, int64_t timeout, const QueryConfig config)
{ 
  if (multipleExperiments)
  {
    std::map<int64_t, const boost::filesystem::path> paths;
    for (const auto& filePath : foundJSONFiles)
    {
      // try to get a numerical ID from the file name
      std::string name = filePath.filename().stem().string();
      auto id = std::stol(name);
      paths.insert({id, filePath});
    }
    addBenchmark(baseline, benchmarkName, paths, fixtureName, config, timeout);
  }
  else
  {
    for (const auto& filePath : foundJSONFiles)
    {
      std::map<int64_t, const boost::filesystem::path> paths;
      paths.insert({0, filePath});
      auto subBenchmarkName = benchmarkName + "_" + filePath.stem().string();
      addBenchmark(baseline, subBenchmarkName, paths, fixtureName, config, timeout);
    }
  }
}


void DynamicBenchmark::addBenchmark(bool baseline,
  std::string benchmarkName,
  std::map<int64_t, const boost::filesystem::path>& paths,
  std::string fixtureName,
  QueryConfig config,
  int64_t timeout)
{
  unsigned int numberOfSamples = 5;

  HL_INFO(benchLogger, (boost::format("adding benchmark %1%") % benchmarkName).str());

  std::map<int64_t, std::string> allQueries;
  std::map<int64_t, unsigned int> expectedCount;
  std::map<int64_t, uint64_t> fixedValues;

  for (auto p : paths)
  {
    auto countPath = p.second.parent_path() /= (p.second.stem().string() + ".count");

    boost::filesystem::ifstream stream;

    stream.open(countPath);
    if (stream.is_open())
    {
      unsigned int tmp;
      stream >> tmp;
      stream.close();
      expectedCount.insert({p.first, tmp});
    }

    stream.open(p.second);
    std::string queryJSON(
      (std::istreambuf_iterator<char>(stream)),
      (std::istreambuf_iterator<char>()));
    stream.close();
    
    allQueries.insert({p.first, queryJSON});
    
    if(baseline)
    {
      double timeVal = 0.0;
      auto timePath = p.second.parent_path() /= (p.second.stem().string() + ".time");
      stream.open(timePath);
      if (stream.is_open())
      {
        stream >> timeVal;
        stream.close();
      }
      if(timeVal > 0.0)
      {
        // since celero uses microseconds an ANNIS milliseconds the value needs to be converted
        fixedValues.insert({p.first, std::llround(timeVal*1000.0)});
      }
      else
      {
        // we would divide by zero later, thus use 1 micro second as smallest value
        fixedValues.insert({p.first, 1});
      }

    }
  }
  std::shared_ptr<::celero::TestFixture> fixture(
    new DynamicCorpusFixture(corpusPath, config, allQueries,
    benchmarkName + " (" + fixtureName + ")", timeout,
    expectedCount));

  if (baseline)
  {
    if(fixedValues.size() > 0)
    {
      std::shared_ptr<::celero::TestFixture> fixedFixture(new FixedValueFixture(fixedValues));
      celero::RegisterBaseline(benchmarkName.c_str(), fixtureName.c_str(), numberOfSamples, 1, 1,
        std::make_shared<DynamicCorpusFixtureFactory>(fixedFixture));      
    }
    else
    {
     celero::RegisterBaseline(benchmarkName.c_str(), fixtureName.c_str(), numberOfSamples, 1, 1,
        std::make_shared<DynamicCorpusFixtureFactory>(fixture));
    }
  }
  else
  {
    celero::RegisterTest(benchmarkName.c_str(), fixtureName.c_str(), numberOfSamples, 1, 1,
      std::make_shared<DynamicCorpusFixtureFactory>(fixture));
  }
}

